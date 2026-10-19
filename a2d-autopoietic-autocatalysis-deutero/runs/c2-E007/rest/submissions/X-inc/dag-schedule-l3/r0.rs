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

    // Validate every dependency before any other validation or computation.
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

    // Build successor lists and the indegrees used by Kahn traversal.
    let mut successors = vec![Vec::<usize>::new(); n];
    let mut indegree = vec![0usize; n];
    for (task_id, task) in tasks.iter().enumerate() {
        indegree[task_id] = task.deps.len();
        for &predecessor in &task.deps {
            successors[predecessor].push(task_id);
        }
    }

    // Use a min-heap so the smallest currently available task is removed first.
    let mut available = std::collections::BinaryHeap::<std::cmp::Reverse<usize>>::new();
    for task_id in 0..n {
        if indegree[task_id] == 0 {
            available.push(std::cmp::Reverse(task_id));
        }
    }

    let mut order = Vec::with_capacity(n);
    let mut remaining_indegree = indegree;
    while let Some(std::cmp::Reverse(task_id)) = available.pop() {
        order.push(task_id);
        for &child in &successors[task_id] {
            remaining_indegree[child] -= 1;
            if remaining_indegree[child] == 0 {
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
        for (task_id, was_removed) in removed.into_iter().enumerate() {
            if !was_removed {
                blocked.push(task_id);
            }
        }
        return Err(Error::Cycle { blocked });
    }

    // Compute bottom levels in reverse Kahn order.
    let mut tails = vec![0u64; n];
    for &task_id in order.iter().rev() {
        let max_successor_tail = successors[task_id]
            .iter()
            .map(|&child| tails[child])
            .max()
            .unwrap_or(0);
        tails[task_id] = tasks[task_id]
            .duration
            .checked_add(max_successor_tail)
            .ok_or(Error::Overflow {
                task: task_id,
                stage: Stage::Tail,
            })?;
    }

    let mut runs: Vec<Option<Run>> = vec![None; n];
    let mut started = vec![false; n];
    let mut completed = vec![false; n];
    let mut running = Vec::<(u64, usize, usize)>::new(); // (finish, task, worker)
    let mut worker_available_at = vec![0u64; workers];
    let mut events = Vec::new();
    let mut completed_predecessors = vec![0usize; n];
    let mut ready = std::collections::BinaryHeap::<(u64, i32, std::cmp::Reverse<usize>)>::new();
    let mut time = 0u64;
    let mut makespan = 0u64;
    let mut completed_count = 0usize;

    // Tasks with no dependencies begin in the ready heap; release eligibility is
    // checked against the current simulation time when selecting each task.
    for task_id in 0..n {
        if tasks[task_id].deps.is_empty() {
            ready.push((
                tails[task_id],
                tasks[task_id].priority,
                std::cmp::Reverse(task_id),
            ));
        }
    }

    while completed_count < n {
        // Complete all running tasks finishing now, in ascending task ID.
        running.sort_unstable_by_key(|&(finish, task_id, _)| (finish, task_id));
        let mut completed_now = Vec::new();
        while running.first().is_some_and(|&(finish, _, _)| finish == time) {
            let (_, task_id, worker) = running.remove(0);
            completed_now.push((task_id, worker));
        }

        for (task_id, worker) in completed_now {
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
                completed_predecessors[child] += 1;
                if completed_predecessors[child] == tasks[child].deps.len() {
                    ready.push((
                        tails[child],
                        tasks[child].priority,
                        std::cmp::Reverse(child),
                    ));
                }
            }
        }

        loop {
            // Find the lowest-ID worker available now.
            let worker = (0..workers).find(|&w| worker_available_at[w] <= time);

            // Select the highest-ranked eligible task, temporarily skipping tasks
            // whose release time has not arrived yet.
            let mut deferred = Vec::new();
            let selected = loop {
                let Some(entry) = ready.pop() else {
                    break None;
                };
                let task_id = entry.2 .0;
                if !started[task_id] && tasks[task_id].release <= time {
                    break Some((task_id, entry));
                }
                deferred.push(entry);
            };
            for entry in deferred {
                ready.push(entry);
            }

            let (Some(worker), Some((task_id, _entry))) = (worker, selected) else {
                // Restore a selected task if there is no worker available.
                if let Some((task_id, entry)) = selected {
                    let _ = task_id;
                    ready.push(entry);
                }
                break;
            };

            started[task_id] = true;
            let finish = time
                .checked_add(tasks[task_id].duration)
                .ok_or(Error::Overflow {
                    task: task_id,
                    stage: Stage::Finish,
                })?;
            let available_at = finish
                .checked_add(turnaround)
                .ok_or(Error::Overflow {
                    task: task_id,
                    stage: Stage::Turnaround,
                })?;

            worker_available_at[worker] = available_at;
            runs[task_id] = Some(Run {
                start: time,
                finish,
                worker,
            });
            events.push(Event {
                time,
                task: task_id,
                worker,
                kind: EventKind::Start,
            });

            if tasks[task_id].duration == 0 {
                // Immediate completion precedes the next admission at this tick.
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
                    completed_predecessors[child] += 1;
                    if completed_predecessors[child] == tasks[child].deps.len() {
                        ready.push((
                            tails[child],
                            tasks[child].priority,
                            std::cmp::Reverse(child),
                        ));
                    }
                }
            } else {
                running.push((finish, task_id, worker));
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

        for &(finish, _, _) in &running {
            consider(finish);
        }
        for task_id in 0..n {
            if !started[task_id] && tasks[task_id].release > time {
                consider(tasks[task_id].release);
            }
        }
        for &available_at in &worker_available_at {
            consider(available_at);
        }

        if let Some(next) = next_time {
            time = next;
        } else {
            // With valid dependencies and all events accounted for, unfinished
            // tasks must have a future release, finish, or worker availability.
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