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
        let mut seen = Vec::with_capacity(task.deps.len());
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
            if seen.contains(&value) {
                return Err(Error::DuplicateDependency {
                    task: task_id,
                    position,
                    value,
                });
            }
            seen.push(value);
        }
    }

    if !(1..=64).contains(&workers) {
        return Err(Error::Workers { count: workers });
    }

    // Build predecessor counts and successor lists.
    let mut indegree = vec![0usize; n];
    let mut children = vec![Vec::new(); n];
    for (task_id, task) in tasks.iter().enumerate() {
        indegree[task_id] = task.deps.len();
        for &dep in &task.deps {
            children[dep].push(task_id);
        }
    }

    // Kahn traversal with the smallest currently available ID selected first.
    let mut available = Vec::new();
    for id in 0..n {
        if indegree[id] == 0 {
            available.push(id);
        }
    }
    let mut order = Vec::with_capacity(n);
    while !available.is_empty() {
        let smallest_pos = available
            .iter()
            .enumerate()
            .min_by_key(|(_, id)| **id)
            .map(|(pos, _)| pos)
            .unwrap();
        let id = available.swap_remove(smallest_pos);
        order.push(id);

        for &child in &children[id] {
            indegree[child] -= 1;
            if indegree[child] == 0 {
                available.push(child);
            }
        }
    }

    if order.len() != n {
        let blocked = (0..n).filter(|&id| indegree[id] != 0).collect();
        return Err(Error::Cycle { blocked });
    }

    // Compute bottom levels in reverse Kahn order.
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
    let mut completed = vec![false; n];
    let mut started = vec![false; n];
    let mut worker_free = vec![0u64; workers];
    let mut running: Vec<(u64, usize, usize)> = Vec::new(); // (finish, task, worker)
    let mut events = Vec::with_capacity(n.saturating_mul(2));
    let mut completed_count = 0usize;
    let mut now = 0u64;
    let mut makespan = 0u64;

    while completed_count < n {
        // Complete all running tasks finishing now, in ascending task ID order.
        running.sort_by_key(|&(finish, task, _)| (finish, task));
        let mut remaining = Vec::with_capacity(running.len());
        for (finish, task, worker) in running.drain(..) {
            if finish == now {
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
                remaining.push((finish, task, worker));
            }
        }
        running = remaining;

        // Keep admitting one task at a time, reevaluating eligibility and rank.
        loop {
            let worker = (0..workers).find(|&w| worker_free[w] <= now);
            let Some(worker) = worker else {
                break;
            };

            let mut best: Option<usize> = None;
            for id in 0..n {
                if started[id] || tasks[id].release > now {
                    continue;
                }
                if !tasks[id].deps.iter().all(|&dep| completed[dep]) {
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
            worker_free[worker] = available_at;
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
            } else {
                running.push((finish, task_id, worker));
            }
        }

        if completed_count == n {
            break;
        }

        // Find the earliest strictly later event that can change the simulation.
        let mut next: Option<u64> = None;
        let mut consider = |candidate: u64| {
            if candidate > now && next.map_or(true, |current| candidate < current) {
                next = Some(candidate);
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
        for &free_at in &worker_free {
            consider(free_at);
        }

        match next {
            Some(time) => now = time,
            None => {
                // With validated acyclic dependencies and finite u64 events,
                // this state is unreachable for a nonempty unfinished schedule.
                return Err(Error::Cycle {
                    blocked: (0..n).filter(|&id| !completed[id]).collect(),
                });
            }
        }
    }

    let runs = runs
        .into_iter()
        .map(|run| run.expect("every task completes"))
        .collect();

    Ok(Schedule {
        runs,
        makespan,
        events,
    })
}