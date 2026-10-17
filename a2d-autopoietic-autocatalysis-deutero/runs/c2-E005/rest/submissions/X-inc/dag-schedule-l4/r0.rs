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

    // Validate every dependency before any other validation.
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

    // Validate resources task by task, preserving the specified error order.
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

    // Build successor lists and indegrees. Dependency list order does not
    // affect Kahn traversal, so successors are sorted by task ID.
    let mut successors = vec![Vec::<usize>::new(); n];
    let mut indegree = vec![0usize; n];
    for (task_id, task) in tasks.iter().enumerate() {
        indegree[task_id] = task.deps.len();
        for &predecessor in &task.deps {
            successors[predecessor].push(task_id);
        }
    }
    for children in &mut successors {
        children.sort_unstable();
    }

    // Kahn traversal, always taking the smallest currently available ID.
    let mut available = Vec::<usize>::new();
    for (task_id, &degree) in indegree.iter().enumerate() {
        if degree == 0 {
            available.push(task_id);
        }
    }
    let mut kahn_order = Vec::with_capacity(n);
    while !available.is_empty() {
        let smallest_pos = available
            .iter()
            .enumerate()
            .min_by_key(|(_, task_id)| **task_id)
            .map(|(position, _)| position)
            .unwrap();
        let task_id = available.swap_remove(smallest_pos);
        kahn_order.push(task_id);

        for &child in &successors[task_id] {
            indegree[child] -= 1;
            if indegree[child] == 0 {
                available.push(child);
            }
        }
    }

    if kahn_order.len() != n {
        let removed = {
            let mut set = vec![false; n];
            for &task_id in &kahn_order {
                set[task_id] = true;
            }
            set
        };
        let blocked = (0..n).filter(|&task_id| !removed[task_id]).collect();
        return Err(Error::Cycle { blocked });
    }

    // Compute bottom levels in reverse Kahn order.
    let mut tails = vec![0u64; n];
    for &task_id in kahn_order.iter().rev() {
        let max_child_tail = successors[task_id]
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

    let mut trace = String::new();
    trace.push_str(&format!(
        "workers={};resources={}\n",
        worker_count,
        capacity.len()
    ));

    let mut runs: Vec<Option<Run>> = vec![None; n];
    let mut started = vec![false; n];
    let mut completed = vec![false; n];
    let mut remaining_predecessors: Vec<usize> =
        tasks.iter().map(|task| task.deps.len()).collect();
    let mut used_resources = vec![0u64; capacity.len()];

    // Worker availability includes its cooldown. A running task records its
    // finish separately, since resources and dependencies release at finish.
    let mut worker_available_at = vec![0u64; worker_count];
    let mut running: Vec<(usize, usize, u64)> = Vec::new(); // task, worker, finish

    let mut now = 0u64;
    let mut completed_count = 0usize;
    let mut makespan = 0u64;

    while completed_count < n {
        // Complete all tasks finishing now, in ascending task ID order.
        let mut finishing: Vec<(usize, usize)> = running
            .iter()
            .filter(|(_, _, finish)| *finish == now)
            .map(|&(task_id, worker, _)| (task_id, worker))
            .collect();
        finishing.sort_unstable_by_key(|&(task_id, _)| task_id);

        for (task_id, worker) in finishing {
            if let Some(pos) = running
                .iter()
                .position(|&(running_task, running_worker, finish)| {
                    running_task == task_id && running_worker == worker && finish == now
                })
            {
                running.swap_remove(pos);
            }

            completed[task_id] = true;
            completed_count += 1;
            makespan = makespan.max(now);

            for resource in 0..capacity.len() {
                used_resources[resource] -= u64::from(tasks[task_id].needs[resource]);
            }
            append_event(
                &mut trace,
                now,
                "end",
                task_id,
                worker,
                &used_resources,
            );

            for &child in &successors[task_id] {
                remaining_predecessors[child] -= 1;
            }
        }

        // Start tasks one at a time, reevaluating eligibility and resource fit
        // after each admission (including immediate zero-duration completions).
        let mut started_any = false;
        loop {
            let has_free_worker = worker_available_at.iter().any(|&time| time <= now);
            if !has_free_worker {
                break;
            }

            let selected = (0..n)
                .filter(|&task_id| {
                    !started[task_id]
                        && tasks[task_id].release <= now
                        && remaining_predecessors[task_id] == 0
                        && tasks[task_id]
                            .needs
                            .iter()
                            .enumerate()
                            .all(|(resource, &need)| {
                                u64::from(need)
                                    <= u64::from(capacity[resource]) - used_resources[resource]
                            })
                })
                .min_by(|&a, &b| {
                    tails[b]
                        .cmp(&tails[a])
                        .then_with(|| tasks[b].priority.cmp(&tasks[a].priority))
                        .then_with(|| a.cmp(&b))
                });

            let Some(task_id) = selected else {
                break;
            };

            let preferred = task_id % worker_count;
            let worker = (0..worker_count)
                .map(|offset| (preferred + offset) % worker_count)
                .find(|&worker| worker_available_at[worker] <= now)
                .unwrap();

            let finish = now.checked_add(tasks[task_id].duration).ok_or(Error::Overflow {
                task: task_id,
                stage: Stage::Finish,
            })?;
            let available_at = finish.checked_add(cooldowns[worker]).ok_or(Error::Overflow {
                task: task_id,
                stage: Stage::Turnaround,
            })?;

            worker_available_at[worker] = available_at;
            started[task_id] = true;
            for resource in 0..capacity.len() {
                used_resources[resource] += u64::from(tasks[task_id].needs[resource]);
            }
            runs[task_id] = Some(Run {
                start: now,
                finish,
                worker,
            });
            running.push((task_id, worker, finish));
            append_event(
                &mut trace,
                now,
                "start",
                task_id,
                worker,
                &used_resources,
            );
            started_any = true;

            if finish == now {
                // Zero-duration tasks complete immediately before another
                // task is selected at this same time.
                let pos = running
                    .iter()
                    .position(|&(running_task, running_worker, running_finish)| {
                        running_task == task_id
                            && running_worker == worker
                            && running_finish == now
                    })
                    .unwrap();
                running.swap_remove(pos);
                completed[task_id] = true;
                completed_count += 1;
                makespan = makespan.max(now);

                for resource in 0..capacity.len() {
                    used_resources[resource] -= u64::from(tasks[task_id].needs[resource]);
                }
                append_event(
                    &mut trace,
                    now,
                    "end",
                    task_id,
                    worker,
                    &used_resources,
                );

                for &child in &successors[task_id] {
                    remaining_predecessors[child] -= 1;
                }
            }
        }

        if completed_count == n {
            break;
        }

        // If no more task can start now, move to the next event time.
        let mut next_time: Option<u64> = None;
        let mut consider = |time: u64| {
            if time > now && next_time.map_or(true, |current| time < current) {
                next_time = Some(time);
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
        for &available_at in &worker_available_at {
            consider(available_at);
        }

        if let Some(time) = next_time {
            now = time;
        } else if !started_any {
            // Acyclic validated input must always have a future event capable
            // of making progress; this branch is unreachable for valid data.
            break;
        }
    }

    let ordered_runs = runs
        .into_iter()
        .map(|run| run.expect("every acyclic task must be started"))
        .collect();

    trace.push_str(&format!("makespan={}\n", makespan));

    Ok(Schedule {
        runs: ordered_runs,
        makespan,
        trace,
    })
}

fn append_event(
    trace: &mut String,
    time: u64,
    kind: &str,
    task_id: usize,
    worker: usize,
    used_resources: &[u64],
) {
    trace.push_str(&format!("{time}|{kind}|{task_id}|{worker}|"));
    if used_resources.is_empty() {
        trace.push('-');
    } else {
        for (index, amount) in used_resources.iter().enumerate() {
            if index != 0 {
                trace.push(',');
            }
            trace.push_str(&amount.to_string());
        }
    }
    trace.push('\n');
}