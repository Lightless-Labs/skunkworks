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

    // Validate dependencies in input order before all other checks.
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

    let mut indegree: Vec<usize> = tasks.iter().map(|t| t.deps.len()).collect();
    let mut children = vec![Vec::new(); n];
    for (task_id, task) in tasks.iter().enumerate() {
        for &pred in &task.deps {
            children[pred].push(task_id);
        }
    }

    // Kahn traversal with the smallest currently available task first.
    let mut available = std::collections::BTreeSet::new();
    for id in 0..n {
        if indegree[id] == 0 {
            available.insert(id);
        }
    }
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
            let mut v = vec![false; n];
            for &id in &order {
                v[id] = true;
            }
            v
        };
        return Err(Error::Cycle {
            blocked: (0..n).filter(|&id| !removed[id]).collect(),
        });
    }

    let mut tail = vec![0u64; n];
    for &id in order.iter().rev() {
        let max_child = children[id]
            .iter()
            .map(|&child| tail[child])
            .max()
            .unwrap_or(0);
        tail[id] = tasks[id]
            .duration
            .checked_add(max_child)
            .ok_or(Error::Overflow {
                task: id,
                stage: Stage::Tail,
            })?;
    }

    let mut runs: Vec<Option<Run>> = vec![None; n];
    let mut started = vec![false; n];
    let mut completed = vec![false; n];
    let mut worker_available = vec![0u64; workers];
    let mut running: Vec<(u64, usize, usize)> = Vec::new(); // (finish, task, worker)
    let mut events = Vec::new();
    let mut completed_count = 0usize;
    let mut now = 0u64;
    let mut makespan = 0u64;

    while completed_count < n {
        // Complete all currently due tasks in ascending task ID order.
        let mut due: Vec<(u64, usize, usize)> = running
            .iter()
            .copied()
            .filter(|&(finish, _, _)| finish == now)
            .collect();
        due.sort_by_key(|&(_, task, _)| task);

        for &(finish, task, worker) in &due {
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
        if !due.is_empty() {
            running.retain(|&(finish, _, _)| finish != now);
        }

        // Start one task at a time, re-ranking after every start.
        loop {
            let worker = (0..workers).find(|&w| worker_available[w] <= now);
            let Some(worker) = worker else {
                break;
            };

            let mut eligible: Vec<usize> = (0..n)
                .filter(|&id| {
                    !started[id]
                        && tasks[id].release <= now
                        && tasks[id].deps.iter().all(|&pred| completed[pred])
                })
                .collect();

            eligible.sort_by(|&a, &b| {
                tail[b]
                    .cmp(&tail[a])
                    .then_with(|| tasks[b].priority.cmp(&tasks[a].priority))
                    .then_with(|| a.cmp(&b))
            });

            let Some(task) = eligible.first().copied() else {
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

            if finish == now {
                completed[task] = true;
                completed_count += 1;
                events.push(Event {
                    time: now,
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

        // Advance to the earliest future event that can change eligibility.
        let mut next_time: Option<u64> = None;
        for &(finish, _, _) in &running {
            if finish > now {
                next_time = Some(next_time.map_or(finish, |x| x.min(finish)));
            }
        }
        for id in 0..n {
            if !started[id] && tasks[id].release > now {
                let t = tasks[id].release;
                next_time = Some(next_time.map_or(t, |x| x.min(t)));
            }
        }
        for &available_at in &worker_available {
            if available_at > now {
                next_time = Some(next_time.map_or(available_at, |x| x.min(available_at)));
            }
        }

        let Some(next) = next_time else {
            // With valid dependencies, this is unreachable unless all tasks are done.
            break;
        };
        now = next;
    }

    let runs = runs
        .into_iter()
        .map(|run| run.expect("every task must run in an acyclic schedule"))
        .collect();

    Ok(Schedule {
        runs,
        makespan,
        events,
    })
}