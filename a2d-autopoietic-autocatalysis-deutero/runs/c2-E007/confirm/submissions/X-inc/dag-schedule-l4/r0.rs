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

fn resource_snapshot(used: &[u64]) -> String {
    if used.is_empty() {
        "-".to_string()
    } else {
        used.iter()
            .map(u64::to_string)
            .collect::<Vec<_>>()
            .join(",")
    }
}

pub fn schedule(
    tasks: &[Task],
    cooldowns: &[u64],
    capacity: &[u32],
) -> Result<Schedule, Error> {
    let n = tasks.len();

    // Validate every dependency in the required order.
    for (task_id, task) in tasks.iter().enumerate() {
        for (position, &dependency) in task.deps.iter().enumerate() {
            if dependency >= n {
                return Err(Error::BadDependency {
                    task: task_id,
                    position,
                    value: dependency,
                });
            }
            if dependency == task_id {
                return Err(Error::SelfDependency {
                    task: task_id,
                    position,
                });
            }
            if task.deps[..position].contains(&dependency) {
                return Err(Error::DuplicateDependency {
                    task: task_id,
                    position,
                    value: dependency,
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

    // Validate resources task by task.
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
        for &dependency in &task.deps {
            children[dependency].push(task_id);
        }
    }

    // Kahn traversal always removes the smallest available task ID.
    let mut available = std::collections::BTreeSet::new();
    for (id, &degree) in indegree.iter().enumerate() {
        if degree == 0 {
            available.insert(id);
        }
    }

    let mut order = Vec::with_capacity(n);
    while let Some(id) = available.pop_first() {
        order.push(id);
        for &child in &children[id] {
            indegree[child] -= 1;
            if indegree[child] == 0 {
                available.insert(child);
            }
        }
    }

    if order.len() != n {
        let mut removed = vec![false; n];
        for &id in &order {
            removed[id] = true;
        }
        return Err(Error::Cycle {
            blocked: (0..n).filter(|&id| !removed[id]).collect(),
        });
    }

    // Bottom levels, computed in reverse Kahn order.
    let mut tail = vec![0u64; n];
    for &id in order.iter().rev() {
        let max_child_tail = children[id]
            .iter()
            .map(|&child| tail[child])
            .max()
            .unwrap_or(0);
        tail[id] = tasks[id]
            .duration
            .checked_add(max_child_tail)
            .ok_or(Error::Overflow {
                task: id,
                stage: Stage::Tail,
            })?;
    }

    let mut trace = format!(
        "workers={};resources={}\n",
        worker_count,
        capacity.len()
    );
    let mut used = vec![0u64; capacity.len()];
    let mut remaining_predecessors: Vec<usize> =
        tasks.iter().map(|task| task.deps.len()).collect();
    let mut started = vec![false; n];
    let mut completed = vec![false; n];
    let mut runs: Vec<Option<Run>> = vec![None; n];

    // Worker availability times; initially all workers are free at time 0.
    let mut worker_free = vec![0u64; worker_count];

    // Running entries are (finish time, task ID, worker ID).
    let mut running: Vec<(u64, usize, usize)> = Vec::new();
    let mut current_time = 0u64;
    let mut completed_count = 0usize;
    let mut makespan = 0u64;

    while completed_count < n {
        // Complete all tasks finishing now in ascending task ID.
        let mut ending: Vec<(u64, usize, usize)> = running
            .iter()
            .copied()
            .filter(|entry| entry.0 == current_time)
            .collect();
        ending.sort_by_key(|entry| entry.1);

        for (finish, id, worker) in ending {
            running.retain(|entry| entry.1 != id);
            completed[id] = true;
            completed_count += 1;
            makespan = makespan.max(finish);

            for (resource, &need) in tasks[id].needs.iter().enumerate() {
                used[resource] -= u64::from(need);
            }

            trace.push_str(&format!(
                "{}|end|{}|{}|{}\n",
                current_time,
                id,
                worker,
                resource_snapshot(&used)
            ));

            for &child in &children[id] {
                remaining_predecessors[child] -= 1;
            }
        }

        // Keep admitting one task at a time, reevaluating after each start.
        loop {
            let chosen = (0..n)
                .filter(|&id| {
                    !started[id]
                        && tasks[id].release <= current_time
                        && remaining_predecessors[id] == 0
                        && tasks[id]
                            .needs
                            .iter()
                            .enumerate()
                            .all(|(r, &need)| {
                                u64::from(need) <= u64::from(capacity[r]) - used[r]
                            })
                })
                .max_by(|&a, &b| {
                    tail[a]
                        .cmp(&tail[b])
                        .then(tasks[a].priority.cmp(&tasks[b].priority))
                        .then_with(|| b.cmp(&a))
                });

            let Some(id) = chosen else {
                break;
            };

            let worker = (0..worker_count)
                .map(|offset| (id % worker_count + offset) % worker_count)
                .find(|&w| worker_free[w] <= current_time);

            let Some(worker) = worker else {
                break;
            };

            let finish = current_time
                .checked_add(tasks[id].duration)
                .ok_or(Error::Overflow {
                    task: id,
                    stage: Stage::Finish,
                })?;
            let turnaround = finish
                .checked_add(cooldowns[worker])
                .ok_or(Error::Overflow {
                    task: id,
                    stage: Stage::Turnaround,
                })?;

            started[id] = true;
            worker_free[worker] = turnaround;
            runs[id] = Some(Run {
                start: current_time,
                finish,
                worker,
            });

            for (resource, &need) in tasks[id].needs.iter().enumerate() {
                used[resource] += u64::from(need);
            }

            trace.push_str(&format!(
                "{}|start|{}|{}|{}\n",
                current_time,
                id,
                worker,
                resource_snapshot(&used)
            ));

            running.push((finish, id, worker));

            // Zero-duration tasks complete immediately before another start.
            if finish == current_time {
                running.retain(|entry| entry.1 != id);
                completed[id] = true;
                completed_count += 1;
                makespan = makespan.max(finish);

                for (resource, &need) in tasks[id].needs.iter().enumerate() {
                    used[resource] -= u64::from(need);
                }

                trace.push_str(&format!(
                    "{}|end|{}|{}|{}\n",
                    current_time,
                    id,
                    worker,
                    resource_snapshot(&used)
                ));

                for &child in &children[id] {
                    remaining_predecessors[child] -= 1;
                }
            }
        }

        if completed_count == n {
            break;
        }

        // Advance to the earliest future event that may change eligibility.
        let mut next_time: Option<u64> = None;
        for &(finish, _, _) in &running {
            if finish > current_time {
                next_time = Some(next_time.map_or(finish, |old| old.min(finish)));
            }
        }
        for id in 0..n {
            if !started[id] && tasks[id].release > current_time {
                let release = tasks[id].release;
                next_time = Some(next_time.map_or(release, |old| old.min(release)));
            }
        }
        for &free_at in &worker_free {
            if free_at > current_time {
                next_time = Some(next_time.map_or(free_at, |old| old.min(free_at)));
            }
        }

        // An unfinished acyclic schedule always has a future release, finish,
        // or cooldown event that can make progress.
        current_time = next_time.expect("unfinished schedule must have a future event");
    }

    trace.push_str(&format!("makespan={}\n", makespan));

    Ok(Schedule {
        runs: runs.into_iter().map(Option::unwrap).collect(),
        makespan,
        trace,
    })
}