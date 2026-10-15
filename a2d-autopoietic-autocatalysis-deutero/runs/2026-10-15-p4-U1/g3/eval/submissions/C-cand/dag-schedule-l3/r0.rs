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
        let mut seen = vec![false; n];
        for (position, &dependency) in task.deps.iter().enumerate() {
            if dependency >= n {
                return Err(Error::BadDependency {
                    task: task_id,
                    position,
                    value: dependency,
                });
            }
            if dependency == task_id {
                return Err(Error::SelfDependency {
                    task: task_id,
                    position,
                });
            }
            if seen[dependency] {
                return Err(Error::DuplicateDependency {
                    task: task_id,
                    position,
                    value: dependency,
                });
            }
            seen[dependency] = true;
        }
    }

    if !(1..=64).contains(&workers) {
        return Err(Error::Workers { count: workers });
    }

    // Build successor lists and indegrees.
    let mut successors = vec![Vec::<usize>::new(); n];
    let mut indegree = vec![0usize; n];
    for (task_id, task) in tasks.iter().enumerate() {
        indegree[task_id] = task.deps.len();
        for &dependency in &task.deps {
            successors[dependency].push(task_id);
        }
    }

    // Kahn traversal, always removing the smallest currently available ID.
    let mut available = std::collections::BinaryHeap::new();
    for (id, &degree) in indegree.iter().enumerate() {
        if degree == 0 {
            available.push(std::cmp::Reverse(id));
        }
    }

    let mut order = Vec::with_capacity(n);
    let mut remaining_indegree = indegree;
    while let Some(std::cmp::Reverse(id)) = available.pop() {
        order.push(id);
        for &child in &successors[id] {
            remaining_indegree[child] -= 1;
            if remaining_indegree[child] == 0 {
                available.push(std::cmp::Reverse(child));
            }
        }
    }

    if order.len() != n {
        let removed: Vec<bool> = {
            let mut flags = vec![false; n];
            for &id in &order {
                flags[id] = true;
            }
            flags
        };
        let blocked = (0..n).filter(|&id| !removed[id]).collect();
        return Err(Error::Cycle { blocked });
    }

    // Compute bottom-level tails in reverse Kahn order.
    let mut tails = vec![0u64; n];
    for &id in order.iter().rev() {
        let max_successor_tail = successors[id]
            .iter()
            .map(|&child| tails[child])
            .max()
            .unwrap_or(0);
        tails[id] = tasks[id]
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
    let mut running: Vec<(u64, usize, usize)> = Vec::new(); // finish, task, worker
    let mut finished_count = 0usize;
    let mut time = 0u64;
    let mut makespan = 0u64;

    while finished_count < n {
        // Complete all tasks finishing now in ascending task ID order.
        let mut completing: Vec<(u64, usize, usize)> = Vec::new();
        let mut still_running = Vec::with_capacity(running.len());
        for item in running.drain(..) {
            if item.0 == time {
                completing.push(item);
            } else {
                still_running.push(item);
            }
        }
        running = still_running;
        completing.sort_by_key(|&(_, task, _)| task);

        for &(finish, task, worker) in &completing {
            completed[task] = true;
            finished_count += 1;
            makespan = makespan.max(finish);
            events.push(Event {
                time: finish,
                task,
                worker,
                kind: EventKind::Finish,
            });
        }

        // Repeatedly admit one task at a time, reevaluating eligibility and rank.
        loop {
            let worker = worker_available
                .iter()
                .position(|&available_at| available_at <= time);
            let Some(worker) = worker else {
                break;
            };

            let mut best: Option<usize> = None;
            for id in 0..n {
                if started[id] || tasks[id].release > time {
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

            let finish = time.checked_add(tasks[task_id].duration).ok_or(Error::Overflow {
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

            if finish == time {
                // Zero-duration completions happen immediately before another start.
                completed[task_id] = true;
                finished_count += 1;
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

        if finished_count == n {
            break;
        }

        // Advance to the next strictly later event that can change the state.
        let mut next_time: Option<u64> = None;
        let mut consider = |candidate: u64| {
            if candidate > time && next_time.map_or(true, |current| candidate < current) {
                next_time = Some(candidate);
            }
        };

        for &(finish, _, _) in &running {
            consider(finish);
        }
        for id in 0..n {
            if !started[id] && tasks[id].release > time {
                consider(tasks[id].release);
            }
        }
        for &available_at in &worker_available {
            consider(available_at);
        }

        time = next_time.expect("unfinished tasks always have a future event");
    }

    let runs = runs
        .into_iter()
        .map(|run| run.expect("every task must run"))
        .collect();

    Ok(Schedule {
        runs,
        makespan,
        events,
    })
}

fn main() {}