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

pub fn schedule(
    tasks: &[Task],
    cooldowns: &[u64],
    capacity: &[u32],
) -> Result<Schedule, Error> {
    use std::collections::BinaryHeap;

    let n = tasks.len();

    // Validate all dependency entries before any other validation.
    for (task_id, task) in tasks.iter().enumerate() {
        let mut seen = vec![false; n];
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
            if seen[value] {
                return Err(Error::DuplicateDependency {
                    task: task_id,
                    position,
                    value,
                });
            }
            seen[value] = true;
        }
    }

    let worker_count = cooldowns.len();
    if !(1..=64).contains(&worker_count) {
        return Err(Error::Workers {
            count: worker_count,
        });
    }

    for (task_id, task) in tasks.iter().enumerate() {
        if task.needs.len() != capacity.len() {
            return Err(Error::ResourceShape {
                task: task_id,
                expected: capacity.len(),
                actual: task.needs.len(),
            });
        }
        for (resource, (&need, &available)) in task.needs.iter().zip(capacity).enumerate() {
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

    let mut indegree = vec![0usize; n];
    let mut children = vec![Vec::<usize>::new(); n];
    for (task_id, task) in tasks.iter().enumerate() {
        indegree[task_id] = task.deps.len();
        for &predecessor in &task.deps {
            children[predecessor].push(task_id);
        }
    }

    let mut available = BinaryHeap::new();
    for task_id in 0..n {
        if indegree[task_id] == 0 {
            available.push(std::cmp::Reverse(task_id));
        }
    }

    let mut order = Vec::with_capacity(n);
    let mut remaining_indegree = indegree.clone();
    while let Some(std::cmp::Reverse(task_id)) = available.pop() {
        order.push(task_id);
        for &child in &children[task_id] {
            remaining_indegree[child] -= 1;
            if remaining_indegree[child] == 0 {
                available.push(std::cmp::Reverse(child));
            }
        }
    }

    if order.len() != n {
        let mut removed = vec![false; n];
        for &task_id in &order {
            removed[task_id] = true;
        }
        return Err(Error::Cycle {
            blocked: (0..n).filter(|&task_id| !removed[task_id]).collect(),
        });
    }

    let mut tails = vec![0u64; n];
    for &task_id in order.iter().rev() {
        let max_child = children[task_id]
            .iter()
            .map(|&child| tails[child])
            .max()
            .unwrap_or(0);
        tails[task_id] = tasks[task_id]
            .duration
            .checked_add(max_child)
            .ok_or(Error::Overflow {
                task: task_id,
                stage: Stage::Tail,
            })?;
    }

    let mut trace = String::new();
    trace.push_str(&format!(
        "workers={};resources={}\n",
        worker_count,
        capacity.len()
    ));

    let mut runs: Vec<Option<Run>> = vec![None; n];
    let mut started = vec![false; n];
    let mut completed = vec![false; n];
    let mut completed_count = 0usize;
    let mut used = vec![0u64; capacity.len()];
    let mut worker_ready = vec![0u64; worker_count];
    let mut running = Vec::<(u64, usize, usize)>::new(); // (finish, task, worker)
    let mut now = 0u64;
    let mut makespan = 0u64;

    while completed_count < n {
        // Complete every task finishing now, in ascending task ID.
        let mut finishing: Vec<(u64, usize, usize)> = running
            .iter()
            .copied()
            .filter(|(finish, _, _)| *finish == now)
            .collect();
        finishing.sort_by_key(|entry| entry.1);

        for (_, task_id, worker) in &finishing {
            for (resource, &need) in tasks[*task_id].needs.iter().enumerate() {
                used[resource] -= u64::from(need);
            }
            completed[*task_id] = true;
            completed_count += 1;
            trace.push_str(&format!(
                "{}|end|{}|{}|{}\n",
                now,
                task_id,
                worker,
                format_resources(&used)
            ));
        }
        if !finishing.is_empty() {
            running.retain(|(finish, _, _)| *finish != now);
        }

        loop {
            let mut best: Option<usize> = None;

            for task_id in 0..n {
                if started[task_id]
                    || tasks[task_id].release > now
                    || !tasks[task_id]
                        .deps
                        .iter()
                        .all(|&predecessor| completed[predecessor])
                {
                    continue;
                }

                let fits = tasks[task_id]
                    .needs
                    .iter()
                    .enumerate()
                    .all(|(r, &need)| u64::from(need) <= u64::from(capacity[r]) - used[r]);
                if !fits {
                    continue;
                }

                let has_worker = worker_ready.iter().any(|&ready| ready <= now);
                if !has_worker {
                    continue;
                }

                let better = match best {
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
                    best = Some(task_id);
                }
            }

            let Some(task_id) = best else {
                break;
            };

            let preferred = task_id % worker_count;
            let worker = (0..worker_count)
                .map(|offset| (preferred + offset) % worker_count)
                .find(|&worker| worker_ready[worker] <= now)
                .expect("worker availability was checked");

            let finish = now
                .checked_add(tasks[task_id].duration)
                .ok_or(Error::Overflow {
                    task: task_id,
                    stage: Stage::Finish,
                })?;
            let ready = finish
                .checked_add(cooldowns[worker])
                .ok_or(Error::Overflow {
                    task: task_id,
                    stage: Stage::Turnaround,
                })?;

            started[task_id] = true;
            worker_ready[worker] = ready;
            for (resource, &need) in tasks[task_id].needs.iter().enumerate() {
                used[resource] += u64::from(need);
            }
            runs[task_id] = Some(Run {
                start: now,
                finish,
                worker,
            });
            makespan = makespan.max(finish);
            trace.push_str(&format!(
                "{}|start|{}|{}|{}\n",
                now,
                task_id,
                worker,
                format_resources(&used)
            ));

            if finish == now {
                for (resource, &need) in tasks[task_id].needs.iter().enumerate() {
                    used[resource] -= u64::from(need);
                }
                completed[task_id] = true;
                completed_count += 1;
                trace.push_str(&format!(
                    "{}|end|{}|{}|{}\n",
                    now,
                    task_id,
                    worker,
                    format_resources(&used)
                ));
            } else {
                running.push((finish, task_id, worker));
            }
        }

        if completed_count == n {
            break;
        }

        let mut next_time = None;
        for &(finish, _, _) in &running {
            if finish > now {
                next_time = Some(next_time.map_or(finish, |current: u64| current.min(finish)));
            }
        }
        for task_id in 0..n {
            if !started[task_id] && tasks[task_id].release > now {
                let release = tasks[task_id].release;
                next_time = Some(next_time.map_or(release, |current| current.min(release)));
            }
        }
        for &ready in &worker_ready {
            if ready > now {
                next_time = Some(next_time.map_or(ready, |current| current.min(ready)));
            }
        }

        now = next_time.expect("unfinished tasks must have a future scheduling event");
    }

    trace.push_str(&format!("makespan={}\n", makespan));

    Ok(Schedule {
        runs: runs.into_iter().map(Option::unwrap).collect(),
        makespan,
        trace,
    })
}

fn format_resources(used: &[u64]) -> String {
    if used.is_empty() {
        return "-".to_owned();
    }
    let mut result = String::new();
    for (index, value) in used.iter().enumerate() {
        if index > 0 {
            result.push(',');
        }
        result.push_str(&value.to_string());
    }
    result
}

fn main() {}