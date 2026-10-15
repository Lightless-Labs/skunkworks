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

    // Dependency validation must be the first validation phase.
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

    if !(1..=64).contains(&cooldowns.len()) {
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

    // Build successor lists and perform the specified smallest-ID Kahn traversal.
    let mut children = vec![Vec::<usize>::new(); n];
    let mut indegree = vec![0usize; n];
    for (task_id, task) in tasks.iter().enumerate() {
        indegree[task_id] = task.deps.len();
        for &predecessor in &task.deps {
            children[predecessor].push(task_id);
        }
    }

    let mut available = Vec::<usize>::new();
    for task_id in 0..n {
        if indegree[task_id] == 0 {
            available.push(task_id);
        }
    }
    let mut kahn_order = Vec::with_capacity(n);
    while !available.is_empty() {
        let smallest_pos = available
            .iter()
            .enumerate()
            .min_by_key(|(_, task_id)| **task_id)
            .map(|(pos, _)| pos)
            .unwrap();
        let task_id = available.swap_remove(smallest_pos);
        kahn_order.push(task_id);
        for &child in &children[task_id] {
            indegree[child] -= 1;
            if indegree[child] == 0 {
                available.push(child);
            }
        }
    }

    if kahn_order.len() != n {
        let removed = {
            let mut removed = vec![false; n];
            for &task_id in &kahn_order {
                removed[task_id] = true;
            }
            removed
        };
        let blocked = (0..n).filter(|&task_id| !removed[task_id]).collect();
        return Err(Error::Cycle { blocked });
    }

    let mut tails = vec![0u64; n];
    for &task_id in kahn_order.iter().rev() {
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

    #[derive(Clone, Copy)]
    struct Running {
        task: usize,
        finish: u64,
        worker: usize,
    }

    let worker_count = cooldowns.len();
    let mut runs: Vec<Option<Run>> = vec![None; n];
    let mut started = vec![false; n];
    let mut completed = vec![false; n];
    let mut running = Vec::<Running>::new();
    let mut worker_available = vec![0u64; worker_count];
    let mut used = vec![0u64; capacity.len()];
    let mut trace = format!("workers={};resources={}\n", worker_count, capacity.len());
    let mut current_time = 0u64;
    let mut completed_count = 0usize;
    let mut makespan = 0u64;

    let append_resources = |trace: &mut String, used: &[u64]| {
        if used.is_empty() {
            trace.push('-');
        } else {
            for (index, amount) in used.iter().enumerate() {
                if index != 0 {
                    trace.push(',');
                }
                trace.push_str(&amount.to_string());
            }
        }
    };

    while completed_count < n {
        // Complete all tasks finishing now, in ascending task-ID order.
        let mut finishing: Vec<usize> = running
            .iter()
            .enumerate()
            .filter(|(_, item)| item.finish == current_time)
            .map(|(index, _)| index)
            .collect();
        finishing.sort_by_key(|&index| running[index].task);

        for index in finishing.into_iter().rev() {
            let item = running.remove(index);
            for (resource, &need) in tasks[item.task].needs.iter().enumerate() {
                used[resource] -= u64::from(need);
            }
            completed[item.task] = true;
            completed_count += 1;
            makespan = makespan.max(item.finish);

            trace.push_str(&format!(
                "{}|end|{}|{}|",
                current_time, item.task, item.worker
            ));
            append_resources(&mut trace, &used);
            trace.push('\n');
        }

        loop {
            let mut eligible: Vec<usize> = (0..n)
                .filter(|&task_id| {
                    if started[task_id] || tasks[task_id].release > current_time {
                        return false;
                    }
                    tasks[task_id]
                        .deps
                        .iter()
                        .all(|&predecessor| completed[predecessor])
                })
                .filter(|&task_id| {
                    tasks[task_id]
                        .needs
                        .iter()
                        .enumerate()
                        .all(|(resource, &need)| {
                            u64::from(need) <= u64::from(capacity[resource]) - used[resource]
                        })
                })
                .collect();

            eligible.sort_by(|&left, &right| {
                tails[right]
                    .cmp(&tails[left])
                    .then_with(|| tasks[right].priority.cmp(&tasks[left].priority))
                    .then_with(|| left.cmp(&right))
            });

            let selected = eligible.into_iter().find_map(|task_id| {
                if running.len() >= worker_count {
                    return None;
                }
                let preferred = task_id % worker_count;
                (0..worker_count)
                    .map(|offset| (preferred + offset) % worker_count)
                    .find(|&worker| worker_available[worker] <= current_time)
                    .map(|worker| (task_id, worker))
            });

            let Some((task_id, worker)) = selected else {
                break;
            };

            let finish = current_time.checked_add(tasks[task_id].duration).ok_or(
                Error::Overflow {
                    task: task_id,
                    stage: Stage::Finish,
                },
            )?;
            let turnaround = finish.checked_add(cooldowns[worker]).ok_or(Error::Overflow {
                task: task_id,
                stage: Stage::Turnaround,
            })?;

            started[task_id] = true;
            worker_available[worker] = turnaround;
            for (resource, &need) in tasks[task_id].needs.iter().enumerate() {
                used[resource] += u64::from(need);
            }
            runs[task_id] = Some(Run {
                start: current_time,
                finish,
                worker,
            });

            trace.push_str(&format!(
                "{}|start|{}|{}|",
                current_time, task_id, worker
            ));
            append_resources(&mut trace, &used);
            trace.push('\n');

            if finish == current_time {
                // A zero-duration task completes immediately before the next admission.
                for (resource, &need) in tasks[task_id].needs.iter().enumerate() {
                    used[resource] -= u64::from(need);
                }
                completed[task_id] = true;
                completed_count += 1;
                makespan = makespan.max(finish);

                trace.push_str(&format!(
                    "{}|end|{}|{}|",
                    current_time, task_id, worker
                ));
                append_resources(&mut trace, &used);
                trace.push('\n');
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
        let mut consider = |time: u64| {
            if time > current_time {
                next_time = Some(next_time.map_or(time, |existing| existing.min(time)));
            }
        };

        for item in &running {
            consider(item.finish);
        }
        for task_id in 0..n {
            if !started[task_id] {
                consider(tasks[task_id].release);
            }
        }
        for &available_at in &worker_available {
            consider(available_at);
        }

        current_time = next_time.expect("unfinished tasks must have a future event");
    }

    let runs = runs.into_iter().map(Option::unwrap).collect();
    trace.push_str(&format!("makespan={}\n", makespan));

    Ok(Schedule {
        runs,
        makespan,
        trace,
    })
}

fn main() {}