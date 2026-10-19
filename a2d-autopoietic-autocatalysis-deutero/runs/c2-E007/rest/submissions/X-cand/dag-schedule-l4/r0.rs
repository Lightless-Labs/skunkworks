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
    let n = tasks.len();

    // Validate all dependency entries before any other validation.
    for (task_id, task) in tasks.iter().enumerate() {
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
            if task.deps[..position].contains(&value) {
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
        return Err(Error::Workers {
            count: worker_count,
        });
    }

    // Validate resource shape and demand in task, then resource, order.
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
    let mut children = vec![Vec::<usize>::new(); n];
    let mut indegree = vec![0usize; n];
    for (task_id, task) in tasks.iter().enumerate() {
        indegree[task_id] = task.deps.len();
        for &predecessor in &task.deps {
            children[predecessor].push(task_id);
        }
    }

    // Deterministic Kahn order: repeatedly remove the smallest available ID.
    let mut available = std::collections::BinaryHeap::new();
    for task_id in 0..n {
        if indegree[task_id] == 0 {
            available.push(std::cmp::Reverse(task_id));
        }
    }

    let mut order = Vec::with_capacity(n);
    let mut remaining_indegree = indegree;
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
        let removed = {
            let mut seen = vec![false; n];
            for &task_id in &order {
                seen[task_id] = true;
            }
            seen
        };
        return Err(Error::Cycle {
            blocked: (0..n).filter(|&task_id| !removed[task_id]).collect(),
        });
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

    let mut runs: Vec<Option<Run>> = vec![None; n];
    let mut started = vec![false; n];
    let mut completed = vec![false; n];
    let mut worker_free = vec![0u64; worker_count];
    let mut used = vec![0u64; capacity.len()];
    let mut running = Vec::<(usize, usize, u64)>::new(); // (task, worker, finish)
    let mut trace = String::new();
    use std::fmt::Write as _;
    writeln!(
        &mut trace,
        "workers={};resources={}",
        worker_count,
        capacity.len()
    )
    .unwrap();

    let mut time = 0u64;
    let mut completed_count = 0usize;
    let mut makespan = 0u64;

    while completed_count < n {
        // Complete every task ending now, in ascending task ID.
        running.sort_by_key(|&(task_id, _, finish)| (finish, task_id));
        let mut kept = Vec::with_capacity(running.len());
        for (task_id, worker, finish) in running.drain(..) {
            if finish == time {
                for (resource, &need) in tasks[task_id].needs.iter().enumerate() {
                    used[resource] -= u64::from(need);
                }
                completed[task_id] = true;
                completed_count += 1;
                makespan = makespan.max(finish);
                write!(
                    &mut trace,
                    "{}|end|{}|{}|",
                    time, task_id, worker
                )
                .unwrap();
                write_resource_snapshot(&mut trace, &used);
                trace.push('\n');
            } else {
                kept.push((task_id, worker, finish));
            }
        }
        running = kept;

        // Start one task at a time, reevaluating after each admission.
        loop {
            let mut best: Option<usize> = None;
            for task_id in 0..n {
                if started[task_id]
                    || tasks[task_id].release > time
                    || !tasks[task_id].deps.iter().all(|&dep| completed[dep])
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

            // Pick the first available worker in the task's cyclic preference order.
            let mut selected_worker = None;
            for offset in 0..worker_count {
                let worker = (task_id % worker_count + offset) % worker_count;
                if worker_free[worker] <= time {
                    selected_worker = Some(worker);
                    break;
                }
            }
            let Some(worker) = selected_worker else {
                break;
            };

            let finish = time
                .checked_add(tasks[task_id].duration)
                .ok_or(Error::Overflow {
                    task: task_id,
                    stage: Stage::Finish,
                })?;
            let available_at = finish
                .checked_add(cooldowns[worker])
                .ok_or(Error::Overflow {
                    task: task_id,
                    stage: Stage::Turnaround,
                })?;

            started[task_id] = true;
            worker_free[worker] = available_at;
            runs[task_id] = Some(Run {
                start: time,
                finish,
                worker,
            });
            for (resource, &need) in tasks[task_id].needs.iter().enumerate() {
                used[resource] += u64::from(need);
            }
            write!(
                &mut trace,
                "{}|start|{}|{}|",
                time, task_id, worker
            )
            .unwrap();
            write_resource_snapshot(&mut trace, &used);
            trace.push('\n');

            if finish == time {
                // Immediate completion of a newly started zero-duration task.
                for (resource, &need) in tasks[task_id].needs.iter().enumerate() {
                    used[resource] -= u64::from(need);
                }
                completed[task_id] = true;
                completed_count += 1;
                makespan = makespan.max(finish);
                write!(
                    &mut trace,
                    "{}|end|{}|{}|",
                    time, task_id, worker
                )
                .unwrap();
                write_resource_snapshot(&mut trace, &used);
                trace.push('\n');
            } else {
                running.push((task_id, worker, finish));
            }
        }

        if completed_count == n {
            break;
        }

        // If nothing can start, advance to the earliest event that may change
        // eligibility, worker availability, or resource availability.
        let mut next_time: Option<u64> = None;
        for &(_, _, finish) in &running {
            if finish > time {
                next_time = Some(next_time.map_or(finish, |t| t.min(finish)));
            }
        }
        for task_id in 0..n {
            if !started[task_id] && tasks[task_id].release > time {
                let release = tasks[task_id].release;
                next_time = Some(next_time.map_or(release, |t| t.min(release)));
            }
        }
        for &free_at in &worker_free {
            if free_at > time {
                next_time = Some(next_time.map_or(free_at, |t| t.min(free_at)));
            }
        }

        match next_time {
            Some(next) => time = next,
            None => break,
        }
    }

    writeln!(&mut trace, "makespan={}", makespan).unwrap();

    Ok(Schedule {
        runs: runs
            .into_iter()
            .map(|run| run.expect("all tasks are started"))
            .collect(),
        makespan,
        trace,
    })
}

fn write_resource_snapshot(output: &mut String, used: &[u64]) {
    if used.is_empty() {
        output.push('-');
    } else {
        for (index, amount) in used.iter().enumerate() {
            if index != 0 {
                output.push(',');
            }
            use std::fmt::Write as _;
            write!(output, "{}", amount).unwrap();
        }
    }
}