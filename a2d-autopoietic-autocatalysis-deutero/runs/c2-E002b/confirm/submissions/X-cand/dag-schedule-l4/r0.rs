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

    // Validate every dependency entry before any other validation.
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

    // Validate resources in task, then resource order.
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

    // Build graph edges from predecessors to successors.
    let mut successors = vec![Vec::<usize>::new(); n];
    let mut indegree = vec![0usize; n];
    for (task_id, task) in tasks.iter().enumerate() {
        indegree[task_id] = task.deps.len();
        for &predecessor in &task.deps {
            successors[predecessor].push(task_id);
        }
    }

    // Kahn traversal with the smallest available task ID selected each time.
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
            .min_by_key(|(_, task_id)| **task_id)
            .map(|(position, _)| position)
            .unwrap();
        let task_id = available.swap_remove(smallest_pos);
        topo.push(task_id);

        for &child in &successors[task_id] {
            indegree[child] -= 1;
            if indegree[child] == 0 {
                available.push(child);
            }
        }
    }

    if topo.len() != n {
        let removed = {
            let mut present = vec![false; n];
            for &task_id in &topo {
                present[task_id] = true;
            }
            present
        };
        return Err(Error::Cycle {
            blocked: (0..n).filter(|&task_id| !removed[task_id]).collect(),
        });
    }

    // Compute bottom levels in reverse Kahn order.
    let mut tails = vec![0u64; n];
    for &task_id in topo.iter().rev() {
        let max_successor_tail = successors[task_id]
            .iter()
            .map(|&child| tails[child])
            .max()
            .unwrap_or(0);
        tails[task_id] = tasks[task_id]
            .duration
            .checked_add(max_successor_tail)
            .ok_or(Error::Overflow {
                task: task_id,
                stage: Stage::Tail,
            })?;
    }

    #[derive(Clone, Copy)]
    struct Running {
        task: usize,
        worker: usize,
        finish: u64,
    }

    let mut runs: Vec<Option<Run>> = vec![None; n];
    let mut started = vec![false; n];
    let mut completed = vec![false; n];
    let mut used_resources = vec![0u64; capacity.len()];
    let mut worker_available = vec![0u64; worker_count];
    let mut running = Vec::<Running>::new();
    let mut completed_count = 0usize;
    let mut current_time = 0u64;
    let mut makespan = 0u64;
    let mut trace = format!("workers={worker_count};resources={}\n", capacity.len());

    while completed_count < n {
        // Complete all tasks finishing now, in ascending task ID order.
        let mut ending: Vec<Running> = running
            .iter()
            .copied()
            .filter(|entry| entry.finish == current_time)
            .collect();
        ending.sort_by_key(|entry| entry.task);

        if !ending.is_empty() {
            running.retain(|entry| entry.finish != current_time);
            for entry in ending {
                let task_id = entry.task;
                completed[task_id] = true;
                completed_count += 1;

                for resource in 0..capacity.len() {
                    used_resources[resource] -= u64::from(tasks[task_id].needs[resource]);
                }

                trace.push_str(&format!(
                    "{}|end|{}|{}|{}\n",
                    current_time,
                    task_id,
                    entry.worker,
                    format_resources(&used_resources)
                ));
            }
        }

        // Start one task at a time, reevaluating eligibility and resource fit.
        loop {
            let mut chosen: Option<usize> = None;
            for task_id in 0..n {
                if started[task_id] || tasks[task_id].release > current_time {
                    continue;
                }
                if !tasks[task_id]
                    .deps
                    .iter()
                    .all(|&predecessor| completed[predecessor])
                {
                    continue;
                }
                if !tasks[task_id]
                    .needs
                    .iter()
                    .enumerate()
                    .all(|(resource, &need)| {
                        u64::from(need)
                            <= u64::from(capacity[resource]) - used_resources[resource]
                    })
                {
                    continue;
                }
                if chosen.map_or(true, |best| {
                    tails[task_id] > tails[best]
                        || (tails[task_id] == tails[best]
                            && (tasks[task_id].priority > tasks[best].priority
                                || (tasks[task_id].priority == tasks[best].priority
                                    && task_id < best)))
                }) {
                    chosen = Some(task_id);
                }
            }

            let Some(task_id) = chosen else {
                break;
            };

            let worker = (0..worker_count)
                .map(|offset| (task_id % worker_count + offset) % worker_count)
                .find(|&worker| worker_available[worker] <= current_time);

            let Some(worker) = worker else {
                break;
            };

            let finish = current_time
                .checked_add(tasks[task_id].duration)
                .ok_or(Error::Overflow {
                    task: task_id,
                    stage: Stage::Finish,
                })?;
            let turnaround = finish
                .checked_add(cooldowns[worker])
                .ok_or(Error::Overflow {
                    task: task_id,
                    stage: Stage::Turnaround,
                })?;

            started[task_id] = true;
            worker_available[worker] = turnaround;
            runs[task_id] = Some(Run {
                start: current_time,
                finish,
                worker,
            });

            for resource in 0..capacity.len() {
                used_resources[resource] += u64::from(tasks[task_id].needs[resource]);
            }

            trace.push_str(&format!(
                "{}|start|{}|{}|{}\n",
                current_time,
                task_id,
                worker,
                format_resources(&used_resources)
            ));

            if finish == current_time {
                // A zero-duration task completes immediately, before the next admission.
                completed[task_id] = true;
                completed_count += 1;
                makespan = makespan.max(finish);

                for resource in 0..capacity.len() {
                    used_resources[resource] -= u64::from(tasks[task_id].needs[resource]);
                }

                trace.push_str(&format!(
                    "{}|end|{}|{}|{}\n",
                    current_time,
                    task_id,
                    worker,
                    format_resources(&used_resources)
                ));
            } else {
                running.push(Running {
                    task: task_id,
                    worker,
                    finish,
                });
                makespan = makespan.max(finish);
            }
        }

        if completed_count == n {
            break;
        }

        // Advance to the next finish, release, or worker availability event.
        let mut next_time: Option<u64> = None;
        for entry in &running {
            if entry.finish > current_time {
                next_time = Some(next_time.map_or(entry.finish, |t| t.min(entry.finish)));
            }
        }
        for task_id in 0..n {
            if !started[task_id] && tasks[task_id].release > current_time {
                let time = tasks[task_id].release;
                next_time = Some(next_time.map_or(time, |t| t.min(time)));
            }
        }
        for &time in &worker_available {
            if time > current_time {
                next_time = Some(next_time.map_or(time, |t| t.min(time)));
            }
        }

        match next_time {
            Some(time) => current_time = time,
            None => {
                // With an acyclic graph and valid capacities this is unreachable.
                // Keep the function total for all specified inputs.
                let blocked = (0..n).filter(|&task_id| !completed[task_id]).collect();
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
        return "-".to_string();
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