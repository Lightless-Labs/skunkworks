#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Task {
    pub duration: u64,
    pub deps: Vec<usize>,
    pub release: u64,
    pub priority: i32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Run {
    pub start: u64,
    pub finish: u64,
    pub worker: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EventKind {
    Start,
    Finish,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Event {
    pub time: u64,
    pub task: usize,
    pub worker: usize,
    pub kind: EventKind,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Schedule {
    pub runs: Vec<Run>,
    pub makespan: u64,
    pub events: Vec<Event>,
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
}

pub fn schedule(tasks: &[Task], workers: usize, turnaround: u64) -> Result<Schedule, Error> {
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

    if !(1..=64).contains(&workers) {
        return Err(Error::Workers { count: workers });
    }

    let mut children = vec![Vec::<usize>::new(); n];
    let mut indegree = vec![0usize; n];
    for (task_id, task) in tasks.iter().enumerate() {
        indegree[task_id] = task.deps.len();
        for &pred in &task.deps {
            children[pred].push(task_id);
        }
    }

    // Kahn traversal always removes the smallest currently available ID.
    let mut remaining_indegree = indegree.clone();
    let mut available = Vec::<usize>::new();
    for id in 0..n {
        if remaining_indegree[id] == 0 {
            available.push(id);
        }
    }

    let mut order = Vec::with_capacity(n);
    while !available.is_empty() {
        let smallest_pos = available
            .iter()
            .enumerate()
            .min_by_key(|&(_, id)| *id)
            .map(|(pos, _)| pos)
            .unwrap();
        let id = available.swap_remove(smallest_pos);
        order.push(id);

        for &child in &children[id] {
            remaining_indegree[child] -= 1;
            if remaining_indegree[child] == 0 {
                available.push(child);
            }
        }
    }

    if order.len() != n {
        let removed = {
            let mut flags = vec![false; n];
            for &id in &order {
                flags[id] = true;
            }
            flags
        };
        let blocked = (0..n).filter(|&id| !removed[id]).collect();
        return Err(Error::Cycle { blocked });
    }

    // Compute bottom-level tails in reverse Kahn order.
    let mut tails = vec![0u64; n];
    for &id in order.iter().rev() {
        let max_child_tail = children[id]
            .iter()
            .map(|&child| tails[child])
            .max()
            .unwrap_or(0);
        tails[id] = tasks[id]
            .duration
            .checked_add(max_child_tail)
            .ok_or(Error::Overflow {
                task: id,
                stage: Stage::Tail,
            })?;
    }

    let mut runs: Vec<Option<Run>> = vec![None; n];
    let mut started = vec![false; n];
    let mut completed = vec![false; n];
    let mut worker_available_at = vec![0u64; workers];
    let mut running = Vec::<(u64, usize, usize)>::new(); // finish, task, worker
    let mut events = Vec::new();
    let mut now = 0u64;
    let mut completed_count = 0usize;
    let mut makespan = 0u64;

    while completed_count < n {
        // Complete all running tasks at this time in ascending task ID.
        let mut due = Vec::new();
        let mut i = 0;
        while i < running.len() {
            if running[i].0 == now {
                due.push(running.swap_remove(i));
            } else {
                i += 1;
            }
        }
        due.sort_by_key(|&(_, task, _)| task);
        for (finish, task, worker) in due {
            completed[task] = true;
            completed_count += 1;
            makespan = makespan.max(finish);
            events.push(Event {
                time: finish,
                task,
                worker,
                kind: EventKind::Finish,
            });
        }

        // Keep admitting one task at a time, reevaluating after each start.
        loop {
            let worker = (0..workers).find(|&w| worker_available_at[w] <= now);
            let Some(worker) = worker else {
                break;
            };

            let mut best: Option<usize> = None;
            for id in 0..n {
                if started[id] || tasks[id].release > now {
                    continue;
                }
                if !tasks[id].deps.iter().all(|&pred| completed[pred]) {
                    continue;
                }

                let better = match best {
                    None => true,
                    Some(current) => {
                        tails[id] > tails[current]
                            || (tails[id] == tails[current]
                                && (tasks[id].priority > tasks[current].priority
                                    || (tasks[id].priority == tasks[current].priority
                                        && id < current)))
                    }
                };
                if better {
                    best = Some(id);
                }
            }

            let Some(task) = best else {
                break;
            };

            let finish = now.checked_add(tasks[task].duration).ok_or(Error::Overflow {
                task,
                stage: Stage::Finish,
            })?;
            let available_at = finish.checked_add(turnaround).ok_or(Error::Overflow {
                task,
                stage: Stage::Turnaround,
            })?;

            started[task] = true;
            worker_available_at[worker] = available_at;
            runs[task] = Some(Run {
                start: now,
                finish,
                worker,
            });
            events.push(Event {
                time: now,
                task,
                worker,
                kind: EventKind::Start,
            });

            if tasks[task].duration == 0 {
                // A zero-duration task completes immediately before another
                // task can be selected at the same time.
                completed[task] = true;
                completed_count += 1;
                makespan = makespan.max(finish);
                events.push(Event {
                    time: finish,
                    task,
                    worker,
                    kind: EventKind::Finish,
                });
            } else {
                running.push((finish, task, worker));
            }
        }

        if completed_count == n {
            break;
        }

        let mut next_time: Option<u64> = None;
        let mut consider = |time: u64| {
            if time > now {
                next_time = Some(next_time.map_or(time, |current| current.min(time)));
            }
        };

        for &(finish, _, _) in &running {
            consider(finish);
        }
        for id in 0..n {
            if !started[id] {
                consider(tasks[id].release);
            }
        }
        for &available_at in &worker_available_at {
            consider(available_at);
        }

        now = next_time.expect("unfinished tasks always have a future event");
    }

    let runs = runs.into_iter().map(Option::unwrap).collect();
    Ok(Schedule {
        runs,
        makespan,
        events,
    })
}