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

    // Validate every dependency entry in the required order.
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

    let mut indegree: Vec<usize> = tasks.iter().map(|t| t.deps.len()).collect();
    let mut children = vec![Vec::<usize>::new(); n];
    for (task_id, task) in tasks.iter().enumerate() {
        for &dep in &task.deps {
            children[dep].push(task_id);
        }
    }

    // Kahn traversal with the smallest available task ID chosen each time.
    let mut available = Vec::new();
    for (id, &degree) in indegree.iter().enumerate() {
        if degree == 0 {
            available.push(id);
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
        let id = available.swap_remove(smallest_pos);
        topo.push(id);
        for &child in &children[id] {
            indegree[child] -= 1;
            if indegree[child] == 0 {
                available.push(child);
            }
        }
    }

    if topo.len() != n {
        let mut removed = vec![false; n];
        for &id in &topo {
            removed[id] = true;
        }
        return Err(Error::Cycle {
            blocked: (0..n).filter(|&id| !removed[id]).collect(),
        });
    }

    let mut tails = vec![0u64; n];
    for &id in topo.iter().rev() {
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

    let mut remaining_deps: Vec<usize> = tasks.iter().map(|t| t.deps.len()).collect();
    let mut started = vec![false; n];
    let mut completed = vec![false; n];
    let mut runs: Vec<Option<Run>> = vec![None; n];
    let mut worker_available = vec![0u64; workers];
    let mut running: Vec<(u64, usize, usize)> = Vec::new(); // (finish, task, worker)
    let mut events = Vec::with_capacity(2 * n);
    let mut now = 0u64;
    let mut completed_count = 0usize;
    let mut makespan = 0u64;

    while completed_count < n {
        // Complete all positive-duration tasks finishing now, in task ID order.
        let mut finishing: Vec<(u64, usize, usize)> = Vec::new();
        let mut still_running = Vec::with_capacity(running.len());
        for entry in running.drain(..) {
            if entry.0 == now {
                finishing.push(entry);
            } else {
                still_running.push(entry);
            }
        }
        running = still_running;
        finishing.sort_by_key(|entry| entry.1);
        for (finish, task_id, worker_id) in finishing {
            completed[task_id] = true;
            completed_count += 1;
            makespan = makespan.max(finish);
            events.push(Event {
                time: finish,
                task: task_id,
                worker: worker_id,
                kind: EventKind::Finish,
            });
            for &child in &children[task_id] {
                remaining_deps[child] -= 1;
            }
        }

        loop {
            let free_worker = worker_available
                .iter()
                .enumerate()
                .find(|(_, ready_at)| **ready_at <= now)
                .map(|(id, _)| id);

            let Some(worker_id) = free_worker else {
                break;
            };

            let mut selected: Option<usize> = None;
            for id in 0..n {
                if started[id]
                    || tasks[id].release > now
                    || remaining_deps[id] != 0
                {
                    continue;
                }

                let better = match selected {
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
                if better {
                    selected = Some(id);
                }
            }

            let Some(task_id) = selected else {
                break;
            };

            let finish = now
                .checked_add(tasks[task_id].duration)
                .ok_or(Error::Overflow {
                    task: task_id,
                    stage: Stage::Finish,
                })?;
            let worker_ready = finish.checked_add(turnaround).ok_or(Error::Overflow {
                task: task_id,
                stage: Stage::Turnaround,
            })?;

            started[task_id] = true;
            worker_available[worker_id] = worker_ready;
            runs[task_id] = Some(Run {
                start: now,
                finish,
                worker: worker_id,
            });
            events.push(Event {
                time: now,
                task: task_id,
                worker: worker_id,
                kind: EventKind::Start,
            });

            if finish == now {
                completed[task_id] = true;
                completed_count += 1;
                makespan = makespan.max(finish);
                events.push(Event {
                    time: finish,
                    task: task_id,
                    worker: worker_id,
                    kind: EventKind::Finish,
                });
                for &child in &children[task_id] {
                    remaining_deps[child] -= 1;
                }
            } else {
                running.push((finish, task_id, worker_id));
            }
        }

        if completed_count == n {
            break;
        }

        let mut next_time: Option<u64> = None;
        let mut consider = |candidate: u64| {
            if candidate > now && next_time.map_or(true, |current| candidate < current) {
                next_time = Some(candidate);
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
        for &ready_at in &worker_available {
            consider(ready_at);
        }

        now = next_time.expect("unfinished valid schedule must have a future event");
    }

    Ok(Schedule {
        runs: runs.into_iter().map(Option::unwrap).collect(),
        makespan,
        events,
    })
}