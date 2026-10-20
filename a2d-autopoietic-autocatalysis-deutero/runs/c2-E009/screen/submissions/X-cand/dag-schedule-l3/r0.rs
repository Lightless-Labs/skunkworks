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

    let mut indegree = Vec::new();
    let mut children: Vec<Vec<usize>> = Vec::new();
    for task in tasks {
        indegree.push(task.deps.len());
        children.push(Vec::new());
    }
    for (task_id, task) in tasks.iter().enumerate() {
        for &predecessor in &task.deps {
            children[predecessor].push(task_id);
        }
    }

    // Kahn traversal, always removing the smallest currently available ID.
    let mut available = std::collections::BinaryHeap::new();
    for (task_id, &degree) in indegree.iter().enumerate() {
        if degree == 0 {
            available.push(std::cmp::Reverse(task_id));
        }
    }

    let mut order = Vec::new();
    while let Some(std::cmp::Reverse(task_id)) = available.pop() {
        order.push(task_id);
        for &child in &children[task_id] {
            indegree[child] -= 1;
            if indegree[child] == 0 {
                available.push(std::cmp::Reverse(child));
            }
        }
    }

    if order.len() != n {
        let mut blocked = Vec::new();
        let mut removed = vec![false; n];
        for &task_id in &order {
            removed[task_id] = true;
        }
        for task_id in 0..n {
            if !removed[task_id] {
                blocked.push(task_id);
            }
        }
        return Err(Error::Cycle { blocked });
    }

    let mut tails = vec![0u64; n];
    for &task_id in order.iter().rev() {
        let mut successor_tail = 0u64;
        for &child in &children[task_id] {
            successor_tail = successor_tail.max(tails[child]);
        }
        tails[task_id] = tasks[task_id]
            .duration
            .checked_add(successor_tail)
            .ok_or(Error::Overflow {
                task: task_id,
                stage: Stage::Tail,
            })?;
    }

    let mut runs: Vec<Option<Run>> = vec![None; n];
    let mut started = vec![false; n];
    let mut completed = vec![false; n];
    let mut remaining_deps: Vec<usize> = tasks.iter().map(|task| task.deps.len()).collect();
    let mut worker_available = vec![0u64; workers];
    let mut running: Vec<(u64, usize, usize)> = Vec::new(); // finish, task, worker
    let mut events = Vec::new();
    let mut now = 0u64;
    let mut completed_count = 0usize;
    let mut makespan = 0u64;

    while completed_count < n {
        // Complete all tasks due now, in ascending task ID order.
        running.sort_by_key(|&(finish, task_id, _)| (finish, task_id));
        let mut i = 0;
        while i < running.len() {
            if running[i].0 == now {
                let (finish, task_id, worker) = running.remove(i);
                completed[task_id] = true;
                completed_count += 1;
                makespan = makespan.max(finish);
                events.push(Event {
                    time: finish,
                    task: task_id,
                    worker,
                    kind: EventKind::Finish,
                });
                for &child in &children[task_id] {
                    remaining_deps[child] -= 1;
                }
            } else {
                i += 1;
            }
        }

        // Keep admitting one task at a time, reevaluating after every start.
        loop {
            let worker = worker_available
                .iter()
                .position(|&available_at| available_at <= now);
            let Some(worker) = worker else {
                break;
            };

            let mut best: Option<usize> = None;
            for task_id in 0..n {
                if started[task_id]
                    || tasks[task_id].release > now
                    || remaining_deps[task_id] != 0
                {
                    continue;
                }
                best = match best {
                    None => Some(task_id),
                    Some(current) => {
                        let candidate_key =
                            (tails[task_id], tasks[task_id].priority, std::cmp::Reverse(task_id));
                        let current_key = (
                            tails[current],
                            tasks[current].priority,
                            std::cmp::Reverse(current),
                        );
                        if candidate_key > current_key {
                            Some(task_id)
                        } else {
                            Some(current)
                        }
                    }
                };
            }

            let Some(task_id) = best else {
                break;
            };

            let finish = now.checked_add(tasks[task_id].duration).ok_or(Error::Overflow {
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
                    time: finish,
                    task: task_id,
                    worker,
                    kind: EventKind::Finish,
                });
                for &child in &children[task_id] {
                    remaining_deps[child] -= 1;
                }
            } else {
                running.push((finish, task_id, worker));
            }
        }

        if completed_count == n {
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
                let release = tasks[task_id].release;
                next_time = Some(next_time.map_or(release, |t| t.min(release)));
            }
        }
        for &available_at in &worker_available {
            if available_at > now {
                next_time = Some(next_time.map_or(available_at, |t| t.min(available_at)));
            }
        }

        // If nothing can make progress, the remaining work is blocked only if
        // the graph was cyclic; that case was already handled above.
        let Some(next) = next_time else {
            break;
        };
        now = next;
    }

    let final_runs = runs.into_iter().map(Option::unwrap).collect();
    Ok(Schedule {
        runs: final_runs,
        makespan,
        events,
    })
}