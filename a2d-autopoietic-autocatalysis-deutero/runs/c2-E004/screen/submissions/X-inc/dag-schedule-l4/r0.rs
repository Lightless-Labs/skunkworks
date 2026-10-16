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

    // Validate every dependency before checking anything else.
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

    for (task_id, task) in tasks.iter().enumerate() {
        if task.needs.len() != capacity.len() {
            return Err(Error::ResourceShape {
                task: task_id,
                expected: capacity.len(),
                actual: task.needs.len(),
            });
        }
        for (resource, (&need, &limit)) in task.needs.iter().zip(capacity).enumerate() {
            if need > limit {
                return Err(Error::ResourceExceeded {
                    task: task_id,
                    resource,
                    need,
                    capacity: limit,
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

    // Kahn traversal with the smallest currently available task first.
    let mut available = Vec::<usize>::new();
    for task_id in 0..n {
        if indegree[task_id] == 0 {
            available.push(task_id);
        }
    }
    let mut topo = Vec::with_capacity(n);
    while !available.is_empty() {
        let smallest_index = available
            .iter()
            .enumerate()
            .min_by_key(|(_, id)| **id)
            .map(|(index, _)| index)
            .unwrap();
        let task_id = available.swap_remove(smallest_index);
        topo.push(task_id);
        for &child in &successors[task_id] {
            indegree[child] -= 1;
            if indegree[child] == 0 {
                available.push(child);
            }
        }
    }

    if topo.len() != n {
        let mut removed = vec![false; n];
        for &task_id in &topo {
            removed[task_id] = true;
        }
        return Err(Error::Cycle {
            blocked: (0..n).filter(|&task_id| !removed[task_id]).collect(),
        });
    }

    let mut tail = vec![0u64; n];
    for &task_id in topo.iter().rev() {
        let max_successor_tail = successors[task_id]
            .iter()
            .map(|&child| tail[child])
            .max()
            .unwrap_or(0);
        tail[task_id] = tasks[task_id]
            .duration
            .checked_add(max_successor_tail)
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
    let mut worker_available = vec![0u64; worker_count];

    #[derive(Clone, Copy)]
    struct Running {
        task: usize,
        finish: u64,
        worker: usize,
    }

    let mut running = Vec::<Running>::new();
    let mut now = 0u64;
    let mut makespan = 0u64;

    while completed_count < n {
        // Complete all tasks finishing now, in ascending task ID.
        let mut finishing: Vec<Running> = running
            .iter()
            .copied()
            .filter(|entry| entry.finish == now)
            .collect();
        finishing.sort_by_key(|entry| entry.task);

        for entry in finishing {
            running.retain(|other| other.task != entry.task);
            completed[entry.task] = true;
            completed_count += 1;
            makespan = makespan.max(entry.finish);

            for (resource, &need) in tasks[entry.task].needs.iter().enumerate() {
                used[resource] -= u64::from(need);
            }

            append_event(&mut trace, now, "end", entry.task, entry.worker, &used);
        }

        // Start one task at a time, reevaluating eligibility after each start.
        loop {
            let mut eligible: Vec<usize> = (0..n)
                .filter(|&task_id| {
                    !started[task_id]
                        && tasks[task_id].release <= now
                        && tasks[task_id]
                            .deps
                            .iter()
                            .all(|&predecessor| completed[predecessor])
                        && tasks[task_id]
                            .needs
                            .iter()
                            .enumerate()
                            .all(|(resource, &need)| {
                                u64::from(need)
                                    <= u64::from(capacity[resource]) - used[resource]
                            })
                })
                .collect();

            eligible.sort_by(|&a, &b| {
                tail[b]
                    .cmp(&tail[a])
                    .then_with(|| tasks[b].priority.cmp(&tasks[a].priority))
                    .then_with(|| a.cmp(&b))
            });

            let Some(task_id) = eligible.first().copied() else {
                break;
            };

            let preferred_worker = task_id % worker_count;
            let worker = (0..worker_count)
                .map(|offset| (preferred_worker + offset) % worker_count)
                .find(|&worker| worker_available[worker] <= now);

            let Some(worker) = worker else {
                break;
            };

            let finish = now
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
            worker_available[worker] = available_at;
            for (resource, &need) in tasks[task_id].needs.iter().enumerate() {
                used[resource] += u64::from(need);
            }
            runs[task_id] = Some(Run {
                start: now,
                finish,
                worker,
            });

            append_event(&mut trace, now, "start", task_id, worker, &used);

            if tasks[task_id].duration == 0 {
                completed[task_id] = true;
                completed_count += 1;
                makespan = makespan.max(finish);
                for (resource, &need) in tasks[task_id].needs.iter().enumerate() {
                    used[resource] -= u64::from(need);
                }
                append_event(&mut trace, now, "end", task_id, worker, &used);
            } else {
                running.push(Running {
                    task: task_id,
                    finish,
                    worker,
                });
            }
        }

        if completed_count == n {
            break;
        }

        let mut next_time: Option<u64> = None;
        for entry in &running {
            if entry.finish > now {
                next_time = Some(next_time.map_or(entry.finish, |current| current.min(entry.finish)));
            }
        }
        for task_id in 0..n {
            if !started[task_id] && tasks[task_id].release > now {
                next_time = Some(
                    next_time.map_or(tasks[task_id].release, |current| {
                        current.min(tasks[task_id].release)
                    }),
                );
            }
        }
        for worker in 0..worker_count {
            if worker_available[worker] > now {
                next_time = Some(
                    next_time.map_or(worker_available[worker], |current| {
                        current.min(worker_available[worker])
                    }),
                );
            }
        }

        // A non-completed task always has a future release, finish, or cooldown
        // event that can change whether it may start.
        now = next_time.expect("unfinished tasks must have a future event");
    }

    trace.push_str(&format!("makespan={}\n", makespan));

    Ok(Schedule {
        runs: runs.into_iter().map(Option::unwrap).collect(),
        makespan,
        trace,
    })
}

fn append_event(
    trace: &mut String,
    time: u64,
    kind: &str,
    task: usize,
    worker: usize,
    used: &[u64],
) {
    trace.push_str(&format!("{time}|{kind}|{task}|{worker}|"));
    if used.is_empty() {
        trace.push('-');
    } else {
        for (index, value) in used.iter().enumerate() {
            if index != 0 {
                trace.push(',');
            }
            trace.push_str(&value.to_string());
        }
    }
    trace.push('\n');
}