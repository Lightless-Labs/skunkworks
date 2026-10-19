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

    // Validate all dependency entries before any other checks.
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

    let mut indegree: Vec<usize> = tasks.iter().map(|task| task.deps.len()).collect();
    let mut children = vec![Vec::new(); n];
    for (task_id, task) in tasks.iter().enumerate() {
        for &dep in &task.deps {
            children[dep].push(task_id);
        }
    }

    // Smallest-ID Kahn traversal.
    let mut available: std::collections::BTreeSet<usize> = indegree
        .iter()
        .enumerate()
        .filter_map(|(id, &degree)| (degree == 0).then_some(id))
        .collect();
    let mut order = Vec::with_capacity(n);

    while let Some(&id) = available.iter().next() {
        available.remove(&id);
        order.push(id);
        for &child in &children[id] {
            indegree[child] -= 1;
            if indegree[child] == 0 {
                available.insert(child);
            }
        }
    }

    if order.len() != n {
        let removed: Vec<bool> = {
            let mut set = vec![false; n];
            for &id in &order {
                set[id] = true;
            }
            set
        };
        return Err(Error::Cycle {
            blocked: (0..n).filter(|&id| !removed[id]).collect(),
        });
    }

    let mut tails = vec![0u64; n];
    for &id in order.iter().rev() {
        let max_successor = children[id]
            .iter()
            .map(|&child| tails[child])
            .max()
            .unwrap_or(0);
        tails[id] = tasks[id]
            .duration
            .checked_add(max_successor)
            .ok_or(Error::Overflow {
                task: id,
                stage: Stage::Tail,
            })?;
    }

    let mut runs: Vec<Option<Run>> = vec![None; n];
    let mut events = Vec::new();
    let mut started = vec![false; n];
    let mut completed = vec![false; n];
    let mut remaining_deps: Vec<usize> = tasks.iter().map(|task| task.deps.len()).collect();

    #[derive(Clone, Copy)]
    struct Running {
        task: usize,
        worker: usize,
        finish: u64,
    }

    let mut running: Vec<Running> = Vec::new();
    let mut worker_available = vec![0u64; workers];
    let mut now = 0u64;
    let mut completed_count = 0usize;
    let mut makespan = 0u64;

    while completed_count < n {
        // Complete all tasks finishing now, in task-ID order.
        running.sort_by_key(|entry| entry.task);
        let mut keep = Vec::with_capacity(running.len());
        for entry in running.drain(..) {
            if entry.finish == now {
                completed[entry.task] = true;
                completed_count += 1;
                makespan = makespan.max(entry.finish);
                events.push(Event {
                    time: now,
                    task: entry.task,
                    worker: entry.worker,
                    kind: EventKind::Finish,
                });
                for &child in &children[entry.task] {
                    remaining_deps[child] -= 1;
                }
            } else {
                keep.push(entry);
            }
        }
        running = keep;

        loop {
            let worker = (0..workers).find(|&id| worker_available[id] <= now);
            let Some(worker) = worker else {
                break;
            };

            let selected = (0..n)
                .filter(|&id| {
                    !started[id]
                        && tasks[id].release <= now
                        && remaining_deps[id] == 0
                })
                .max_by(|&a, &b| {
                    tails[a]
                        .cmp(&tails[b])
                        .then_with(|| tasks[a].priority.cmp(&tasks[b].priority))
                        .then_with(|| b.cmp(&a))
                });

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

            if finish == now {
                completed[task_id] = true;
                completed_count += 1;
                makespan = makespan.max(finish);
                events.push(Event {
                    time: now,
                    task: task_id,
                    worker,
                    kind: EventKind::Finish,
                });
                for &child in &children[task_id] {
                    remaining_deps[child] -= 1;
                }
            } else {
                running.push(Running {
                    task: task_id,
                    worker,
                    finish,
                });
            }
        }

        if completed_count == n {
            break;
        }

        let mut next_time: Option<u64> = None;
        for entry in &running {
            if entry.finish > now {
                next_time = Some(next_time.map_or(entry.finish, |t| t.min(entry.finish)));
            }
        }
        for (id, task) in tasks.iter().enumerate() {
            if !started[id] && task.release > now {
                next_time = Some(next_time.map_or(task.release, |t| t.min(task.release)));
            }
        }
        for &available_at in &worker_available {
            if available_at > now {
                next_time = Some(next_time.map_or(available_at, |t| t.min(available_at)));
            }
        }

        if let Some(next) = next_time {
            now = next;
        } else {
            // All remaining work is blocked only if a cycle exists, already ruled out.
            break;
        }
    }

    Ok(Schedule {
        runs: runs
            .into_iter()
            .map(|run| run.expect("acyclic tasks are all scheduled"))
            .collect(),
        makespan,
        events,
    })
}