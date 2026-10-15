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

    if cooldowns.is_empty() || cooldowns.len() > 64 {
        return Err(Error::Workers {
            count: cooldowns.len(),
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

    let mut children = vec![Vec::<usize>::new(); n];
    let mut indegree = vec![0usize; n];
    for (task_id, task) in tasks.iter().enumerate() {
        indegree[task_id] = task.deps.len();
        for &predecessor in &task.deps {
            children[predecessor].push(task_id);
        }
    }

    // Kahn traversal with the smallest available task ID chosen each time.
    let mut available = Vec::<usize>::new();
    for (task_id, &degree) in indegree.iter().enumerate() {
        if degree == 0 {
            available.push(task_id);
        }
    }

    let mut topo = Vec::with_capacity(n);
    while !available.is_empty() {
        let smallest_pos = available
            .iter()
            .enumerate()
            .min_by_key(|(_, id)| **id)
            .map(|(pos, _)| pos)
            .unwrap();
        let task_id = available.swap_remove(smallest_pos);
        topo.push(task_id);

        for &child in &children[task_id] {
            indegree[child] -= 1;
            if indegree[child] == 0 {
                available.push(child);
            }
        }
    }

    if topo.len() != n {
        let blocked = (0..n)
            .filter(|&task_id| indegree[task_id] > 0)
            .collect();
        return Err(Error::Cycle { blocked });
    }

    // Compute tails in reverse Kahn order.
    let mut tail = vec![0u64; n];
    for &task_id in topo.iter().rev() {
        let successor_tail = children[task_id]
            .iter()
            .map(|&child| tail[child])
            .max()
            .unwrap_or(0);
        tail[task_id] = tasks[task_id]
            .duration
            .checked_add(successor_tail)
            .ok_or(Error::Overflow {
                task: task_id,
                stage: Stage::Tail,
            })?;
    }

    #[derive(Clone, Copy)]
    struct Running {
        task: usize,
        finish: u64,
        worker: usize,
    }

    let worker_count = cooldowns.len();
    let resource_count = capacity.len();
    let mut started = vec![false; n];
    let mut completed = vec![false; n];
    let mut remaining_predecessors: Vec<usize> =
        tasks.iter().map(|task| task.deps.len()).collect();
    let mut worker_available_at = vec![0u64; worker_count];
    let mut running = Vec::<Running>::new();
    let mut used = vec![0u64; resource_count];
    let mut runs: Vec<Option<Run>> = vec![None; n];
    let mut trace = format!("workers={worker_count};resources={resource_count}\n");
    let mut now = 0u64;
    let mut makespan = 0u64;
    let mut completed_count = 0usize;

    loop {
        // Complete every task finishing now, in ascending task ID order.
        let mut finishing: Vec<Running> = running
            .iter()
            .copied()
            .filter(|entry| entry.finish == now)
            .collect();
        finishing.sort_by_key(|entry| entry.task);

        if !finishing.is_empty() {
            running.retain(|entry| entry.finish != now);
            for entry in finishing {
                let task_id = entry.task;
                for resource in 0..resource_count {
                    used[resource] -= u64::from(tasks[task_id].needs[resource]);
                }
                completed[task_id] = true;
                completed_count += 1;
                makespan = makespan.max(now);

                trace.push_str(&format!(
                    "{}|end|{}|{}|{}\n",
                    now,
                    task_id,
                    entry.worker,
                    format_resources(&used)
                ));

                for &child in &children[task_id] {
                    remaining_predecessors[child] -= 1;
                }
            }
        }

        // Reevaluate eligibility after each individual admission.
        loop {
            let mut best: Option<usize> = None;
            for task_id in 0..n {
                if started[task_id]
                    || tasks[task_id].release > now
                    || remaining_predecessors[task_id] != 0
                {
                    continue;
                }

                let fits = (0..resource_count).all(|resource| {
                    u64::from(tasks[task_id].needs[resource])
                        <= u64::from(capacity[resource]) - used[resource]
                });
                if !fits {
                    continue;
                }

                let better = match best {
                    None => true,
                    Some(current) => {
                        tail[task_id] > tail[current]
                            || (tail[task_id] == tail[current]
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

            // Choose a free worker only after selecting the task.
            let preferred = task_id % worker_count;
            let worker = (0..worker_count)
                .map(|offset| (preferred + offset) % worker_count)
                .find(|&worker| worker_available_at[worker] <= now);

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
            worker_available_at[worker] = available_at;
            for resource in 0..resource_count {
                used[resource] += u64::from(tasks[task_id].needs[resource]);
            }
            runs[task_id] = Some(Run {
                start: now,
                finish,
                worker,
            });
            running.push(Running {
                task: task_id,
                finish,
                worker,
            });

            trace.push_str(&format!(
                "{}|start|{}|{}|{}\n",
                now,
                task_id,
                worker,
                format_resources(&used)
            ));

            if finish == now {
                // Immediate completion of a zero-duration task.
                running.retain(|entry| !(entry.task == task_id && entry.finish == now));
                for resource in 0..resource_count {
                    used[resource] -= u64::from(tasks[task_id].needs[resource]);
                }
                completed[task_id] = true;
                completed_count += 1;
                makespan = makespan.max(now);
                trace.push_str(&format!(
                    "{}|end|{}|{}|{}\n",
                    now,
                    task_id,
                    worker,
                    format_resources(&used)
                ));
                for &child in &children[task_id] {
                    remaining_predecessors[child] -= 1;
                }
            }
        }

        if completed_count == n {
            break;
        }

        // Advance to the earliest event that can change eligibility or capacity.
        let mut next_time: Option<u64> = None;
        for entry in &running {
            if entry.finish > now {
                next_time = Some(next_time.map_or(entry.finish, |t| t.min(entry.finish)));
            }
        }
        for task_id in 0..n {
            if !started[task_id] && tasks[task_id].release > now {
                next_time = Some(
                    next_time.map_or(tasks[task_id].release, |t| t.min(tasks[task_id].release)),
                );
            }
        }
        for &available_at in &worker_available_at {
            if available_at > now {
                next_time = Some(next_time.map_or(available_at, |t| t.min(available_at)));
            }
        }

        match next_time {
            Some(time) => now = time,
            None => {
                // With a validated acyclic graph, progress must eventually be possible.
                // This guard avoids an accidental infinite loop if invariants are violated.
                let blocked = (0..n).filter(|&id| !completed[id]).collect();
                return Err(Error::Cycle { blocked });
            }
        }
    }

    trace.push_str(&format!("makespan={makespan}\n"));

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
        if index != 0 {
            result.push(',');
        }
        result.push_str(&value.to_string());
    }
    result
}