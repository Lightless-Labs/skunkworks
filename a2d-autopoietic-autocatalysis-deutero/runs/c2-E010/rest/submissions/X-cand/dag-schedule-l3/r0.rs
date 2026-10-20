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
    BadDependency { task: usize, position: usize, value: usize },
    SelfDependency { task: usize, position: usize },
    DuplicateDependency { task: usize, position: usize, value: usize },
    Cycle { blocked: Vec<usize> },
    Workers { count: usize },
    Overflow { task: usize, stage: Stage },
}

pub fn schedule(tasks: &[Task], workers: usize, turnaround: u64) -> Result<Schedule, Error> {
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

    // Min-heap behavior via Reverse.
    let mut ready = std::collections::BinaryHeap::new();
    for id in 0..n {
        if indegree[id] == 0 {
            ready.push(std::cmp::Reverse(id));
        }
    }

    let mut topo = Vec::with_capacity(n);
    let mut remaining_indegree = indegree.clone();
    while let Some(std::cmp::Reverse(id)) = ready.pop() {
        topo.push(id);
        for &child in &children[id] {
            remaining_indegree[child] -= 1;
            if remaining_indegree[child] == 0 {
                ready.push(std::cmp::Reverse(child));
            }
        }
    }

    if topo.len() != n {
        let mut blocked = Vec::new();
        let mut removed = vec![false; n];
        for &id in &topo {
            removed[id] = true;
        }
        for id in 0..n {
            if !removed[id] {
                blocked.push(id);
            }
        }
        return Err(Error::Cycle { blocked });
    }

    let mut tail = vec![0u64; n];
    for &id in topo.iter().rev() {
        let mut max_successor_tail = 0u64;
        for &child in &children[id] {
            max_successor_tail = max_successor_tail.max(tail[child]);
        }
        tail[id] = tasks[id]
            .duration
            .checked_add(max_successor_tail)
            .ok_or(Error::Overflow {
                task: id,
                stage: Stage::Tail,
            })?;
    }

    let mut runs: Vec<Option<Run>> = vec![None; n];
    let mut events = Vec::with_capacity(n.saturating_mul(2));
    let mut completed = vec![false; n];
    let mut started = vec![false; n];
    let mut worker_available = vec![0u64; workers];
    let mut running = Vec::<(u64, usize, usize)>::new(); // (finish, task, worker)
    let mut now = 0u64;
    let mut finished_count = 0usize;
    let mut makespan = 0u64;

    while finished_count < n {
        // Complete all positive-duration tasks finishing now, in task ID order.
        running.sort_unstable_by_key(|&(finish, task, _)| (finish, task));
        let mut i = 0;
        while i < running.len() {
            if running[i].0 == now {
                let (_, task, worker) = running.remove(i);
                completed[task] = true;
                finished_count += 1;
                makespan = makespan.max(now);
                events.push(Event {
                    time: now,
                    task,
                    worker,
                    kind: EventKind::Finish,
                });
            } else {
                i += 1;
            }
        }

        loop {
            let available_worker = worker_available
                .iter()
                .position(|&available_at| available_at <= now);
            let Some(worker) = available_worker else {
                break;
            };

            let mut selected: Option<usize> = None;
            for task_id in 0..n {
                if started[task_id]
                    || tasks[task_id].release > now
                    || !tasks[task_id].deps.iter().all(|&pred| completed[pred])
                {
                    continue;
                }
                let better = match selected {
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
                    selected = Some(task_id);
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
            let available_at = finish.checked_add(turnaround).ok_or(Error::Overflow {
                task: task_id,
                stage: Stage::Turnaround,
            })?;

            started[task_id] = true;
            worker_available[worker] = available_at;
            runs[task_id] = Some(Run {
                start: now,
                finish,
                worker,
            });
            events.push(Event {
                time: now,
                task: task_id,
                worker,
                kind: EventKind::Start,
            });

            if tasks[task_id].duration == 0 {
                completed[task_id] = true;
                finished_count += 1;
                makespan = makespan.max(now);
                events.push(Event {
                    time: now,
                    task: task_id,
                    worker,
                    kind: EventKind::Finish,
                });
            } else {
                running.push((finish, task_id, worker));
            }
        }

        if finished_count == n {
            break;
        }

        let mut next_time: Option<u64> = None;
        for &(finish, _, _) in &running {
            if finish > now {
                next_time = Some(next_time.map_or(finish, |t| t.min(finish)));
            }
        }
        for task_id in 0..n {
            if !started[task_id] && tasks[task_id].release > now {
                let t = tasks[task_id].release;
                next_time = Some(next_time.map_or(t, |cur| cur.min(t)));
            }
        }
        for &available_at in &worker_available {
            if available_at > now {
                next_time = Some(next_time.map_or(available_at, |cur| cur.min(available_at)));
            }
        }

        match next_time {
            Some(t) => now = t,
            None => break,
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
