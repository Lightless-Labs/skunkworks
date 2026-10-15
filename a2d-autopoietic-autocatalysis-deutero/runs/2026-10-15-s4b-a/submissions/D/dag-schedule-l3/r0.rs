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

    // Validate every dependency in the specified order before other checks.
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

    let mut indegree = vec![0usize; n];
    let mut children = vec![Vec::new(); n];
    for (task_id, task) in tasks.iter().enumerate() {
        indegree[task_id] = task.deps.len();
        for &dep in &task.deps {
            children[dep].push(task_id);
        }
    }

    // Kahn traversal with the smallest available task ID selected first.
    let mut available: Vec<usize> = (0..n).filter(|&i| indegree[i] == 0).collect();
    let mut order = Vec::with_capacity(n);
    while !available.is_empty() {
        let smallest_pos = available
            .iter()
            .enumerate()
            .min_by_key(|&(_, task_id)| *task_id)
            .map(|(pos, _)| pos)
            .unwrap();
        let task_id = available.swap_remove(smallest_pos);
        order.push(task_id);

        for &child in &children[task_id] {
            indegree[child] -= 1;
            if indegree[child] == 0 {
                available.push(child);
            }
        }
    }

    if order.len() != n {
        let removed = {
            let mut bits = vec![false; n];
            for &id in &order {
                bits[id] = true;
            }
            bits
        };
        let blocked = (0..n).filter(|&id| !removed[id]).collect();
        return Err(Error::Cycle { blocked });
    }

    let mut tail = vec![0u64; n];
    for &task_id in order.iter().rev() {
        let max_child_tail = children[task_id]
            .iter()
            .map(|&child| tail[child])
            .max()
            .unwrap_or(0);
        tail[task_id] = tasks[task_id]
            .duration
            .checked_add(max_child_tail)
            .ok_or(Error::Overflow {
                task: task_id,
                stage: Stage::Tail,
            })?;
    }

    let mut remaining_deps: Vec<usize> = tasks.iter().map(|t| t.deps.len()).collect();
    let mut started = vec![false; n];
    let mut runs: Vec<Option<Run>> = vec![None; n];
    let mut worker_available = vec![0u64; workers];
    let mut running: Vec<(u64, usize, usize)> = Vec::new(); // (finish, task, worker)
    let mut events = Vec::with_capacity(n.saturating_mul(2));
    let mut now = 0u64;
    let mut completed_count = 0usize;
    let mut makespan = 0u64;

    while completed_count < n {
        // Complete all running tasks at this time in task ID order.
        let mut finishing: Vec<(u64, usize, usize)> = Vec::new();
        let mut still_running = Vec::with_capacity(running.len());
        for item in running.drain(..) {
            if item.0 == now {
                finishing.push(item);
            } else {
                still_running.push(item);
            }
        }
        running = still_running;
        finishing.sort_by_key(|&(_, task_id, _)| task_id);

        for &(finish, task_id, worker_id) in &finishing {
            events.push(Event {
                time: finish,
                task: task_id,
                worker: worker_id,
                kind: EventKind::Finish,
            });
            completed_count += 1;
            makespan = makespan.max(finish);
            for &child in &children[task_id] {
                remaining_deps[child] -= 1;
            }
        }

        loop {
            let worker_id = match (0..workers).find(|&w| worker_available[w] <= now) {
                Some(w) => w,
                None => break,
            };

            let mut best: Option<usize> = None;
            for task_id in 0..n {
                if started[task_id]
                    || tasks[task_id].release > now
                    || remaining_deps[task_id] != 0
                {
                    continue;
                }
                let is_better = match best {
                    None => true,
                    Some(current) => {
                        tail[task_id] > tail[current]
                            || (tail[task_id] == tail[current]
                                && (tasks[task_id].priority > tasks[current].priority
                                    || (tasks[task_id].priority == tasks[current].priority
                                        && task_id < current)))
                    }
                };
                if is_better {
                    best = Some(task_id);
                }
            }

            let task_id = match best {
                Some(id) => id,
                None => break,
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
            worker_available[worker_id] = available_at;
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
                events.push(Event {
                    time: finish,
                    task: task_id,
                    worker: worker_id,
                    kind: EventKind::Finish,
                });
                completed_count += 1;
                makespan = makespan.max(finish);
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

        // A remaining unstarted task can always eventually become admissible
        // after its dependencies finish. This fallback is unreachable for a
        // valid acyclic graph, but avoids a loop if internal state is inconsistent.
        now = match next_time {
            Some(t) => t,
            None => break,
        };
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