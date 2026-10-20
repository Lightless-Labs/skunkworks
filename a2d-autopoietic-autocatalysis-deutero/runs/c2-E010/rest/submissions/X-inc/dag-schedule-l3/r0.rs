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

    // Validate every dependency before checking any other condition.
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

    let mut indegree = vec![0usize; n];
    let mut children = vec![Vec::<usize>::new(); n];
    for (task_id, task) in tasks.iter().enumerate() {
        indegree[task_id] = task.deps.len();
        for &dep in &task.deps {
            children[dep].push(task_id);
        }
    }

    // Sorted available IDs; insertion keeps the smallest currently ready ID first.
    let mut available = Vec::<usize>::new();
    for id in 0..n {
        if indegree[id] == 0 {
            available.push(id);
        }
    }

    let mut topo = Vec::with_capacity(n);
    while !available.is_empty() {
        let id = available.remove(0);
        topo.push(id);
        for &child in &children[id] {
            indegree[child] -= 1;
            if indegree[child] == 0 {
                let pos = available.binary_search(&child).unwrap_or_else(|p| p);
                available.insert(pos, child);
            }
        }
    }

    if topo.len() != n {
        let removed = {
            let mut mark = vec![false; n];
            for &id in &topo {
                mark[id] = true;
            }
            mark
        };
        return Err(Error::Cycle {
            blocked: (0..n).filter(|&id| !removed[id]).collect(),
        });
    }

    let mut tail = vec![0u64; n];
    for &id in topo.iter().rev() {
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

    let mut runs: Vec<Option<Run>> = vec![None; n];
    let mut started = vec![false; n];
    let mut completed = vec![false; n];
    let mut remaining_deps: Vec<usize> = tasks.iter().map(|t| t.deps.len()).collect();

    let mut worker_available = vec![0u64; workers];
    let mut running = Vec::<(u64, usize, usize)>::new(); // (finish, task, worker)
    let mut events = Vec::new();
    let mut now = 0u64;
    let mut completed_count = 0usize;
    let mut makespan = 0u64;

    while completed_count < n {
        // Complete all running work at this time in task-ID order.
        running.sort_by_key(|&(finish, task, _)| (finish, task));
        let mut i = 0;
        while i < running.len() {
            if running[i].0 == now {
                let (_, task, worker) = running.remove(i);
                completed[task] = true;
                completed_count += 1;
                makespan = makespan.max(now);
                events.push(Event {
                    time: now,
                    task,
                    worker,
                    kind: EventKind::Finish,
                });
                for &child in &children[task] {
                    remaining_deps[child] -= 1;
                }
            } else {
                i += 1;
            }
        }

        loop {
            let worker = match worker_available.iter().position(|&time| time <= now) {
                Some(worker) => worker,
                None => break,
            };

            let mut best: Option<usize> = None;
            for id in 0..n {
                if started[id]
                    || tasks[id].release > now
                    || remaining_deps[id] != 0
                {
                    continue;
                }
                best = match best {
                    None => Some(id),
                    Some(old) => {
                        let better = tail[id] > tail[old]
                            || (tail[id] == tail[old]
                                && (tasks[id].priority > tasks[old].priority
                                    || (tasks[id].priority == tasks[old].priority
                                        && id < old)));
                        Some(if better { id } else { old })
                    }
                };
            }

            let task = match best {
                Some(task) => task,
                None => break,
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
            worker_available[worker] = available_at;
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
                completed[task] = true;
                completed_count += 1;
                makespan = makespan.max(now);
                events.push(Event {
                    time: now,
                    task,
                    worker,
                    kind: EventKind::Finish,
                });
                for &child in &children[task] {
                    remaining_deps[child] -= 1;
                }
            } else {
                running.push((finish, task, worker));
            }
        }

        if completed_count == n {
            break;
        }

        let mut next_time: Option<u64> = None;
        for &(finish, _, _) in &running {
            if finish > now {
                next_time = Some(next_time.map_or(finish, |v| v.min(finish)));
            }
        }
        for id in 0..n {
            if !started[id] && tasks[id].release > now {
                let release = tasks[id].release;
                next_time = Some(next_time.map_or(release, |v| v.min(release)));
            }
        }
        for &available_at in &worker_available {
            if available_at > now {
                next_time = Some(next_time.map_or(available_at, |v| v.min(available_at)));
            }
        }

        // With unfinished tasks, a future event must exist: releases, finishes,
        // or cooldown expiries. All values are bounded u64 event times.
        now = next_time.expect("unfinished tasks must have a future scheduling event");
    }

    Ok(Schedule {
        runs: runs.into_iter().map(Option::unwrap).collect(),
        makespan,
        events,
    })
}