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

pub fn schedule(
    tasks: &[Task],
    workers: usize,
    turnaround: u64,
) -> Result<Schedule, Error> {
    let n = tasks.len();

    // Validate dependency entries in the specified order.
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

    if !(1..=64).contains(&workers) {
        return Err(Error::Workers { count: workers });
    }

    // Build successor lists and indegrees.
    let mut children = vec![Vec::new(); n];
    let mut indegree = vec![0usize; n];
    for (task_id, task) in tasks.iter().enumerate() {
        indegree[task_id] = task.deps.len();
        for &dep in &task.deps {
            children[dep].push(task_id);
        }
    }

    // Kahn traversal, always removing the smallest currently available ID.
    let mut available = std::collections::BinaryHeap::new();
    for task_id in 0..n {
        if indegree[task_id] == 0 {
            available.push(std::cmp::Reverse(task_id));
        }
    }

    let mut topo = Vec::with_capacity(n);
    while let Some(std::cmp::Reverse(task_id)) = available.pop() {
        topo.push(task_id);
        for &child in &children[task_id] {
            indegree[child] -= 1;
            if indegree[child] == 0 {
                available.push(std::cmp::Reverse(child));
            }
        }
    }

    if topo.len() != n {
        let mut blocked = Vec::new();
        for (task_id, &degree) in indegree.iter().enumerate() {
            if degree != 0 {
                blocked.push(task_id);
            }
        }
        return Err(Error::Cycle { blocked });
    }

    // Compute bottom levels in reverse topological order.
    let mut tails = vec![0u64; n];
    for &task_id in topo.iter().rev() {
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

    let mut runs: Vec<Option<Run>> = vec![None; n];
    let mut events = Vec::new();
    let mut completed = vec![false; n];
    let mut started = vec![false; n];
    let mut worker_free_at = vec![0u64; workers];
    let mut running: Vec<(u64, usize, usize)> = Vec::new(); // (finish, task, worker)
    let mut remaining = n;
    let mut now = 0u64;
    let mut makespan = 0u64;

    while remaining > 0 {
        // Complete all running tasks finishing now, sorted by task ID.
        let mut finishing: Vec<(u64, usize, usize)> = running
            .iter()
            .copied()
            .filter(|(finish, _, _)| *finish == now)
            .collect();
        finishing.sort_by_key(|&(_, task_id, _)| task_id);

        if !finishing.is_empty() {
            running.retain(|(finish, _, _)| *finish != now);
            for (finish, task_id, worker_id) in finishing {
                completed[task_id] = true;
                remaining -= 1;
                makespan = makespan.max(finish);
                events.push(Event {
                    time: finish,
                    task: task_id,
                    worker: worker_id,
                    kind: EventKind::Finish,
                });
            }
        }

        // Start tasks one at a time, reevaluating eligibility and ranking.
        loop {
            let free_worker = (0..workers).find(|&worker_id| worker_free_at[worker_id] <= now);
            let Some(worker_id) = free_worker else {
                break;
            };

            let mut best: Option<usize> = None;
            for task_id in 0..n {
                if started[task_id]
                    || tasks[task_id].release > now
                    || !tasks[task_id]
                        .deps
                        .iter()
                        .all(|&dep| completed[dep])
                {
                    continue;
                }

                let replace = match best {
                    None => true,
                    Some(current) => {
                        tails[task_id] > tails[current]
                            || (tails[task_id] == tails[current]
                                && (tasks[task_id].priority > tasks[current].priority
                                    || (tasks[task_id].priority == tasks[current].priority
                                        && task_id < current)))
                    }
                };
                if replace {
                    best = Some(task_id);
                }
            }

            let Some(task_id) = best else {
                break;
            };

            let finish = now
                .checked_add(tasks[task_id].duration)
                .ok_or(Error::Overflow {
                    task: task_id,
                    stage: Stage::Finish,
                })?;
            let worker_available = finish
                .checked_add(turnaround)
                .ok_or(Error::Overflow {
                    task: task_id,
                    stage: Stage::Turnaround,
                })?;

            started[task_id] = true;
            worker_free_at[worker_id] = worker_available;
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

            if tasks[task_id].duration == 0 {
                // Complete immediately before selecting another task.
                completed[task_id] = true;
                remaining -= 1;
                makespan = makespan.max(finish);
                events.push(Event {
                    time: finish,
                    task: task_id,
                    worker: worker_id,
                    kind: EventKind::Finish,
                });
            } else {
                running.push((finish, task_id, worker_id));
            }
        }

        if remaining == 0 {
            break;
        }

        // Advance to the earliest relevant strictly later event.
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
        for &free_at in &worker_free_at {
            if free_at > now {
                next_time = Some(next_time.map_or(free_at, |t| t.min(free_at)));
            }
        }

        // Since all tasks are acyclic, an unstarted task must eventually become
        // eligible; this fallback only handles a theoretical eventless state.
        let Some(next) = next_time else {
            break;
        };
        now = next;
    }

    let runs = runs
        .into_iter()
        .map(|run| run.expect("all tasks in an acyclic graph are scheduled"))
        .collect();

    Ok(Schedule {
        runs,
        makespan,
        events,
    })
}