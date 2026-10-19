use std::collections::BTreeSet;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Task {
    pub duration: u64,
    pub deps: Vec<usize>,
    pub release: u64,
    pub priority: i32,
    pub needs: Vec<u32>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Run {
    pub start: u64,
    pub finish: u64,
    pub worker: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Schedule {
    pub runs: Vec<Run>,
    pub makespan: u64,
    pub trace: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Stage {
    Tail,
    Finish,
    Turnaround,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Error {
    BadDependency {
        task: usize,
        position: usize,
        value: usize,
    },
    SelfDependency {
        task: usize,
        position: usize,
    },
    DuplicateDependency {
        task: usize,
        position: usize,
        value: usize,
    },
    Cycle {
        blocked: Vec<usize>,
    },
    Workers {
        count: usize,
    },
    Overflow {
        task: usize,
        stage: Stage,
    },
    ResourceShape {
        task: usize,
        expected: usize,
        actual: usize,
    },
    ResourceExceeded {
        task: usize,
        resource: usize,
        need: u32,
        capacity: u32,
    },
}

fn resource_string(used: &[u128]) -> String {
    if used.is_empty() {
        return "-".to_owned();
    }

    let mut result = String::new();
    for (i, amount) in used.iter().enumerate() {
        if i != 0 {
            result.push(',');
        }
        result.push_str(&amount.to_string());
    }
    result
}

pub fn schedule(
    tasks: &[Task],
    cooldowns: &[u64],
    capacity: &[u32],
) -> Result<Schedule, Error> {
    let n = tasks.len();

    // Validate all dependency entries before any other checks.
    for (task_id, task) in tasks.iter().enumerate() {
        let mut seen = BTreeSet::new();
        for (position, &value) in task.deps.iter().enumerate() {
            if value >= n {
                return Err(Error::BadDependency {
                    task: task_id,
                    position,
                    value,
                });
            }
            if value == task_id {
                return Err(Error::SelfDependency {
                    task: task_id,
                    position,
                });
            }
            if !seen.insert(value) {
                return Err(Error::DuplicateDependency {
                    task: task_id,
                    position,
                    value,
                });
            }
        }
    }

    if cooldowns.is_empty() || cooldowns.len() > 64 {
        return Err(Error::Workers {
            count: cooldowns.len(),
        });
    }

    // Validate resource shapes and demands in task order.
    for (task_id, task) in tasks.iter().enumerate() {
        if task.needs.len() != capacity.len() {
            return Err(Error::ResourceShape {
                task: task_id,
                expected: capacity.len(),
                actual: task.needs.len(),
            });
        }
        for (resource, (&need, &available)) in
            task.needs.iter().zip(capacity.iter()).enumerate()
        {
            if need > available {
                return Err(Error::ResourceExceeded {
                    task: task_id,
                    resource,
                    need,
                    capacity: available,
                });
            }
        }
    }

    // Build predecessor counts and successor lists.
    let mut indegree = vec![0usize; n];
    let mut children = vec![Vec::<usize>::new(); n];
    for (task_id, task) in tasks.iter().enumerate() {
        indegree[task_id] = task.deps.len();
        for &predecessor in &task.deps {
            children[predecessor].push(task_id);
        }
    }

    // Kahn traversal, always removing the smallest available ID.
    let mut available = BTreeSet::new();
    for task_id in 0..n {
        if indegree[task_id] == 0 {
            available.insert(task_id);
        }
    }

    let mut order = Vec::with_capacity(n);
    let mut remaining_indegree = indegree;
    while let Some(task_id) = available.pop_first() {
        order.push(task_id);
        for &child in &children[task_id] {
            remaining_indegree[child] -= 1;
            if remaining_indegree[child] == 0 {
                available.insert(child);
            }
        }
    }

    if order.len() != n {
        let removed = {
            let mut flags = vec![false; n];
            for &task_id in &order {
                flags[task_id] = true;
            }
            flags
        };
        let blocked = (0..n).filter(|&task_id| !removed[task_id]).collect();
        return Err(Error::Cycle { blocked });
    }

    // Compute bottom levels in reverse Kahn order.
    let mut tails = vec![0u64; n];
    for &task_id in order.iter().rev() {
        let max_child_tail = children[task_id]
            .iter()
            .map(|&child| tails[child])
            .max()
            .unwrap_or(0);
        tails[task_id] = tasks[task_id]
            .duration
            .checked_add(max_child_tail)
            .ok_or(Error::Overflow {
                task: task_id,
                stage: Stage::Tail,
            })?;
    }

    let worker_count = cooldowns.len();
    let mut runs: Vec<Option<Run>> = vec![None; n];
    let mut started = vec![false; n];
    let mut completed = vec![false; n];
    let mut remaining_predecessors: Vec<usize> =
        tasks.iter().map(|task| task.deps.len()).collect();

    let mut worker_available_at = vec![0u64; worker_count];
    let mut worker_busy = vec![false; worker_count];
    let mut running = Vec::<(usize, usize, u64)>::new(); // (task, worker, finish)
    let mut used = vec![0u128; capacity.len()];
    let mut trace = format!("workers={};resources={}\n", worker_count, capacity.len());
    let mut now = 0u64;
    let mut makespan = 0u64;
    let mut completed_count = 0usize;

    while completed_count < n {
        // Complete every task finishing now, in ascending task ID.
        let mut ending: Vec<(usize, usize, u64)> = running
            .iter()
            .copied()
            .filter(|&(_, _, finish)| finish == now)
            .collect();
        ending.sort_by_key(|&(task_id, _, _)| task_id);

        for &(task_id, worker, finish) in &ending {
            for (resource, &need) in tasks[task_id].needs.iter().enumerate() {
                used[resource] -= need as u128;
            }

            trace.push_str(&format!(
                "{}|end|{}|{}|{}\n",
                now,
                task_id,
                worker,
                resource_string(&used)
            ));

            completed[task_id] = true;
            completed_count += 1;
            makespan = makespan.max(finish);
            worker_busy[worker] = false;

            for &child in &children[task_id] {
                remaining_predecessors[child] -= 1;
            }
        }

        if !ending.is_empty() {
            running.retain(|&(_, _, finish)| finish != now);
        }

        // Start tasks one at a time, reevaluating eligibility and fit after each.
        loop {
            let mut selected: Option<usize> = None;

            for task_id in 0..n {
                if started[task_id]
                    || tasks[task_id].release > now
                    || remaining_predecessors[task_id] != 0
                {
                    continue;
                }

                let fits = tasks[task_id]
                    .needs
                    .iter()
                    .enumerate()
                    .all(|(resource, &need)| {
                        (need as u128) <= (capacity[resource] as u128) - used[resource]
                    });
                if !fits {
                    continue;
                }

                let better = match selected {
                    None => true,
                    Some(current) => {
                        tails[task_id] > tails[current]
                            || (tails[task_id] == tails[current]
                                && (tasks[task_id].priority > tasks[current].priority
                                    || (tasks[task_id].priority == tasks[current].priority
                                        && task_id < current)))
                    }
                };
                if better {
                    selected = Some(task_id);
                }
            }

            let Some(task_id) = selected else {
                break;
            };

            // Choose the first available worker in the task's cyclic preference order.
            let preferred = task_id % worker_count;
            let worker = (0..worker_count)
                .map(|offset| (preferred + offset) % worker_count)
                .find(|&worker| !worker_busy[worker] && worker_available_at[worker] <= now);

            let Some(worker) = worker else {
                break;
            };

            let finish = now
                .checked_add(tasks[task_id].duration)
                .ok_or(Error::Overflow {
                    task: task_id,
                    stage: Stage::Finish,
                })?;
            let available_at = finish.checked_add(cooldowns[worker]).ok_or(Error::Overflow {
                task: task_id,
                stage: Stage::Turnaround,
            })?;

            started[task_id] = true;
            worker_busy[worker] = true;
            worker_available_at[worker] = available_at;

            for (resource, &need) in tasks[task_id].needs.iter().enumerate() {
                used[resource] += need as u128;
            }

            runs[task_id] = Some(Run {
                start: now,
                finish,
                worker,
            });

            trace.push_str(&format!(
                "{}|start|{}|{}|{}\n",
                now,
                task_id,
                worker,
                resource_string(&used)
            ));

            if finish == now {
                // Zero-duration tasks complete immediately before another admission.
                for (resource, &need) in tasks[task_id].needs.iter().enumerate() {
                    used[resource] -= need as u128;
                }
                trace.push_str(&format!(
                    "{}|end|{}|{}|{}\n",
                    now,
                    task_id,
                    worker,
                    resource_string(&used)
                ));

                completed[task_id] = true;
                completed_count += 1;
                makespan = makespan.max(finish);
                worker_busy[worker] = false;

                for &child in &children[task_id] {
                    remaining_predecessors[child] -= 1;
                }
            } else {
                running.push((task_id, worker, finish));
            }
        }

        if completed_count == n {
            break;
        }

        // Advance to the next event that can change eligibility or availability.
        let mut next_time: Option<u64> = None;
        let mut consider = |time: u64| {
            if time > now {
                next_time = Some(next_time.map_or(time, |current| current.min(time)));
            }
        };

        for &(_, _, finish) in &running {
            consider(finish);
        }
        for task_id in 0..n {
            if !started[task_id] && tasks[task_id].release > now {
                consider(tasks[task_id].release);
            }
        }
        for worker in 0..worker_count {
            if !worker_busy[worker] && worker_available_at[worker] > now {
                consider(worker_available_at[worker]);
            }
        }

        let Some(next) = next_time else {
            // A validated acyclic graph always has a future progress event unless
            // all work is complete; retain a defensive non-panicking exit.
            return Err(Error::Cycle {
                blocked: (0..n).filter(|&task_id| !completed[task_id]).collect(),
            });
        };
        now = next;
    }

    trace.push_str(&format!("makespan={}\n", makespan));

    let runs = runs
        .into_iter()
        .map(|run| run.expect("every task is started exactly once"))
        .collect();

    Ok(Schedule {
        runs,
        makespan,
        trace,
    })
}