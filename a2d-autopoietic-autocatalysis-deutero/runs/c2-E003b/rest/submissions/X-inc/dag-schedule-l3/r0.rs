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

    // Validate every dependency before checking worker count or computing anything else.
    for (task_id, task) in tasks.iter().enumerate() {
        for (position, &dep) in task.deps.iter().enumerate() {
            if dep >= n {
                return Err(Error::BadDependency {
                    task: task_id,
                    position,
                    value: dep,
                });
            }
            if dep == task_id {
                return Err(Error::SelfDependency {
                    task: task_id,
                    position,
                });
            }
            if task.deps[..position].contains(&dep) {
                return Err(Error::DuplicateDependency {
                    task: task_id,
                    position,
                    value: dep,
                });
            }
        }
    }

    if !(1..=64).contains(&workers) {
        return Err(Error::Workers { count: workers });
    }

    let mut successors = vec![Vec::<usize>::new(); n];
    let mut indegree = vec![0usize; n];
    for (task_id, task) in tasks.iter().enumerate() {
        indegree[task_id] = task.deps.len();
        for &dep in &task.deps {
            successors[dep].push(task_id);
        }
    }

    // Kahn traversal, always taking the smallest currently available ID.
    let mut ready = Vec::new();
    for id in 0..n {
        if indegree[id] == 0 {
            ready.push(id);
        }
    }

    let mut kahn_order = Vec::with_capacity(n);
    while !ready.is_empty() {
        let smallest_pos = ready
            .iter()
            .enumerate()
            .min_by_key(|(_, id)| **id)
            .map(|(pos, _)| pos)
            .unwrap();
        let id = ready.swap_remove(smallest_pos);
        kahn_order.push(id);

        for &child in &successors[id] {
            indegree[child] -= 1;
            if indegree[child] == 0 {
                ready.push(child);
            }
        }
    }

    if kahn_order.len() != n {
        let removed = {
            let mut flags = vec![false; n];
            for &id in &kahn_order {
                flags[id] = true;
            }
            flags
        };
        let blocked = (0..n).filter(|&id| !removed[id]).collect();
        return Err(Error::Cycle { blocked });
    }

    // Compute bottom levels in reverse topological order.
    let mut tails = vec![0u64; n];
    for &id in kahn_order.iter().rev() {
        let max_successor_tail = successors[id]
            .iter()
            .map(|&child| tails[child])
            .max()
            .unwrap_or(0);
        tails[id] = tasks[id]
            .duration
            .checked_add(max_successor_tail)
            .ok_or(Error::Overflow {
                task: id,
                stage: Stage::Tail,
            })?;
    }

    let mut runs: Vec<Option<Run>> = vec![None; n];
    let mut events = Vec::new();
    let mut completed = vec![false; n];
    let mut started = vec![false; n];
    let mut running: Vec<Option<(usize, u64)>> = vec![None; workers];
    let mut worker_available_at = vec![0u64; workers];
    let mut remaining_predecessors: Vec<usize> =
        tasks.iter().map(|task| task.deps.len()).collect();
    let mut makespan = 0u64;
    let mut completed_count = 0usize;
    let mut time = 0u64;

    while completed_count < n {
        // Complete all positive-duration jobs finishing now, in task ID order.
        let mut finishing = Vec::new();
        for worker in 0..workers {
            if let Some((task_id, finish)) = running[worker] {
                if finish == time {
                    finishing.push((task_id, worker));
                }
            }
        }
        finishing.sort_by_key(|&(task_id, _)| task_id);

        for (task_id, worker) in finishing {
            running[worker] = None;
            completed[task_id] = true;
            completed_count += 1;
            makespan = makespan.max(time);

            events.push(Event {
                time,
                task: task_id,
                worker,
                kind: EventKind::Finish,
            });

            for &child in &successors[task_id] {
                remaining_predecessors[child] -= 1;
            }
        }

        loop {
            let worker = (0..workers)
                .find(|&w| running[w].is_none() && worker_available_at[w] <= time);
            let Some(worker) = worker else {
                break;
            };

            let mut selected: Option<usize> = None;
            for id in 0..n {
                if started[id]
                    || tasks[id].release > time
                    || remaining_predecessors[id] != 0
                {
                    continue;
                }

                let is_better = match selected {
                    None => true,
                    Some(current) => {
                        tails[id] > tails[current]
                            || (tails[id] == tails[current]
                                && tasks[id].priority > tasks[current].priority)
                            || (tails[id] == tails[current]
                                && tasks[id].priority == tasks[current].priority
                                && id < current)
                    }
                };
                if is_better {
                    selected = Some(id);
                }
            }

            let Some(task_id) = selected else {
                break;
            };

            let finish = time.checked_add(tasks[task_id].duration).ok_or(Error::Overflow {
                task: task_id,
                stage: Stage::Finish,
            })?;
            let available_at = finish.checked_add(turnaround).ok_or(Error::Overflow {
                task: task_id,
                stage: Stage::Turnaround,
            })?;

            started[task_id] = true;
            runs[task_id] = Some(Run {
                start: time,
                finish,
                worker,
            });
            worker_available_at[worker] = available_at;

            events.push(Event {
                time,
                task: task_id,
                worker,
                kind: EventKind::Start,
            });

            if tasks[task_id].duration == 0 {
                completed[task_id] = true;
                completed_count += 1;
                makespan = makespan.max(time);

                events.push(Event {
                    time,
                    task: task_id,
                    worker,
                    kind: EventKind::Finish,
                });

                for &child in &successors[task_id] {
                    remaining_predecessors[child] -= 1;
                }
            } else {
                running[worker] = Some((task_id, finish));
            }
        }

        if completed_count == n {
            break;
        }

        let mut next_time: Option<u64> = None;
        let mut consider = |candidate: u64| {
            if candidate > time && next_time.map_or(true, |current| candidate < current) {
                next_time = Some(candidate);
            }
        };

        for worker in 0..workers {
            if let Some((_, finish)) = running[worker] {
                consider(finish);
            } else if worker_available_at[worker] > time {
                consider(worker_available_at[worker]);
            }
        }
        for id in 0..n {
            if !started[id] && tasks[id].release > time {
                consider(tasks[id].release);
            }
        }

        if let Some(next) = next_time {
            time = next;
        } else {
            // A task remains, so absent a future event the input could not make progress.
            // Acyclic dependencies and finite releases ensure this is unreachable.
            break;
        }
    }

    let runs = runs
        .into_iter()
        .map(|run| run.expect("acyclic tasks are all scheduled"))
        .collect();

    Ok(Schedule {
        runs,
        makespan,
        events,
    })
}