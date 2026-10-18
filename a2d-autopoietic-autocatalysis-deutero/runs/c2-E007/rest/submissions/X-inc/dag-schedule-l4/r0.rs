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
    BadDependency { task: usize, position: usize, value: usize },
    SelfDependency { task: usize, position: usize },
    DuplicateDependency { task: usize, position: usize, value: usize },
    Cycle { blocked: Vec<usize> },
    Workers { count: usize },
    Overflow { task: usize, stage: Stage },
    ResourceShape { task: usize, expected: usize, actual: usize },
    ResourceExceeded { task: usize, resource: usize, need: u32, capacity: u32 },
}

pub fn schedule(
    tasks: &[Task],
    cooldowns: &[u64],
    capacity: &[u32],
) -> Result<Schedule, Error> {
    let n = tasks.len();

    // Validate dependency entries in the required order.
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

    let worker_count = cooldowns.len();
    if !(1..=64).contains(&worker_count) {
        return Err(Error::Workers { count: worker_count });
    }

    // Validate resource shapes and demands task by task.
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

    // Build successor lists and indegrees.
    let mut successors = vec![Vec::<usize>::new(); n];
    let mut indegree = vec![0usize; n];
    for (task_id, task) in tasks.iter().enumerate() {
        indegree[task_id] = task.deps.len();
        for &predecessor in &task.deps {
            successors[predecessor].push(task_id);
        }
    }

    // Kahn traversal always removes the smallest available task ID.
    let mut available = BTreeSet::new();
    for task_id in 0..n {
        if indegree[task_id] == 0 {
            available.insert(task_id);
        }
    }

    let mut order = Vec::with_capacity(n);
    while let Some(task_id) = available.pop_first() {
        order.push(task_id);
        for &child in &successors[task_id] {
            indegree[child] -= 1;
            if indegree[child] == 0 {
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
    let mut tail = vec![0u64; n];
    for &task_id in order.iter().rev() {
        let max_child_tail = successors[task_id]
            .iter()
            .map(|&child| tail[child])
            .max()
            .unwrap_or(0);
        tail[task_id] = tasks[task_id]
            .duration
            .checked_add(max_child_tail)
            .ok_or(Error::Overflow {
                task: task_id,
                stage: Stage::Tail,
            })?;
    }

    let mut runs: Vec<Option<Run>> = vec![None; n];
    let mut started = vec![false; n];
    let mut completed = vec![false; n];
    let mut used = vec![0u64; capacity.len()];
    let mut worker_available = vec![0u64; worker_count];
    let mut running = Vec::<(u64, usize, usize)>::new(); // (finish, task, worker)
    let mut time = 0u64;
    let mut completed_count = 0usize;
    let mut makespan = 0u64;

    let mut trace = String::new();
    trace.push_str(&format!(
        "workers={};resources={}\n",
        worker_count,
        capacity.len()
    ));

    loop {
        // Complete every task finishing now in ascending task ID order.
        let mut finishing: Vec<(u64, usize, usize)> = running
            .iter()
            .copied()
            .filter(|&(finish, _, _)| finish == time)
            .collect();
        finishing.sort_by_key(|&(_, task_id, _)| task_id);

        if !finishing.is_empty() {
            running.retain(|&(finish, _, _)| finish != time);
            for &(finish, task_id, worker) in &finishing {
                completed[task_id] = true;
                completed_count += 1;
                makespan = makespan.max(finish);
                for (resource, &need) in tasks[task_id].needs.iter().enumerate() {
                    used[resource] -= u64::from(need);
                }
                trace.push_str(&format!(
                    "{}|end|{}|{}|{}\n",
                    time,
                    task_id,
                    worker,
                    format_used(&used)
                ));
            }
        }

        // Repeatedly admit one task, reevaluating eligibility and ranking.
        loop {
            let mut best: Option<usize> = None;
            for task_id in 0..n {
                if started[task_id] || tasks[task_id].release > time {
                    continue;
                }
                if !tasks[task_id]
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
                    .all(|(resource, &need)| {
                        u64::from(need) <= u64::from(capacity[resource]) - used[resource]
                    });
                if !fits {
                    continue;
                }

                let better = match best {
                    None => true,
                    Some(current) => {
                        tail[task_id] > tail[current]
                            || (tail[task_id] == tail[current]
                                && tasks[task_id].priority > tasks[current].priority)
                            || (tail[task_id] == tail[current]
                                && tasks[task_id].priority == tasks[current].priority
                                && task_id < current)
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
                .find(|&worker| worker_available[worker] <= time);

            let Some(worker) = worker else {
                break;
            };

            let finish = time.checked_add(tasks[task_id].duration).ok_or(Error::Overflow {
                task: task_id,
                stage: Stage::Finish,
            })?;
            let available_at = finish.checked_add(cooldowns[worker]).ok_or(Error::Overflow {
                task: task_id,
                stage: Stage::Turnaround,
            })?;

            started[task_id] = true;
            worker_available[worker] = available_at;
            for (resource, &need) in tasks[task_id].needs.iter().enumerate() {
                used[resource] += u64::from(need);
            }
            runs[task_id] = Some(Run {
                start: time,
                finish,
                worker,
            });
            trace.push_str(&format!(
                "{}|start|{}|{}|{}\n",
                time,
                task_id,
                worker,
                format_used(&used)
            ));

            if tasks[task_id].duration == 0 {
                completed[task_id] = true;
                completed_count += 1;
                makespan = makespan.max(finish);
                for (resource, &need) in tasks[task_id].needs.iter().enumerate() {
                    used[resource] -= u64::from(need);
                }
                trace.push_str(&format!(
                    "{}|end|{}|{}|{}\n",
                    time,
                    task_id,
                    worker,
                    format_used(&used)
                ));
            } else {
                running.push((finish, task_id, worker));
            }
        }

        if completed_count == n {
            break;
        }

        // Advance to the earliest strictly later event that can change the
        // availability of a task or worker.
        let mut next_time: Option<u64> = None;
        let mut consider = |candidate: u64| {
            if candidate > time {
                next_time = Some(next_time.map_or(candidate, |current| current.min(candidate)));
            }
        };

        for &(finish, _, _) in &running {
            consider(finish);
        }
        for task_id in 0..n {
            if !started[task_id] && tasks[task_id].release > time {
                consider(tasks[task_id].release);
            }
        }
        for worker in 0..worker_count {
            if worker_available[worker] > time {
                consider(worker_available[worker]);
            }
        }

        match next_time {
            Some(next) => time = next,
            None => break,
        }
    }

    trace.push_str(&format!("makespan={}\n", makespan));

    let runs = runs
        .into_iter()
        .map(|run| run.expect("all tasks must have been started"))
        .collect();

    Ok(Schedule {
        runs,
        makespan,
        trace,
    })
}

fn format_used(used: &[u64]) -> String {
    if used.is_empty() {
        return "-".to_string();
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