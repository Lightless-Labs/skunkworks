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

    // Validate every dependency entry in the specified order before other work.
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

    // Construct successor lists and indegrees.
    let mut successors = vec![Vec::<usize>::new(); n];
    let mut indegree = vec![0usize; n];
    for (task_id, task) in tasks.iter().enumerate() {
        indegree[task_id] = task.deps.len();
        for &dep in &task.deps {
            successors[dep].push(task_id);
        }
    }

    // Kahn traversal with the smallest currently available task first.
    let mut available = Vec::<usize>::new();
    for (task_id, &degree) in indegree.iter().enumerate() {
        if degree == 0 {
            available.push(task_id);
        }
    }

    let mut kahn_order = Vec::with_capacity(n);
    while !available.is_empty() {
        let smallest_pos = available
            .iter()
            .enumerate()
            .min_by_key(|(_, task_id)| **task_id)
            .map(|(position, _)| position)
            .unwrap();
        let task_id = available.swap_remove(smallest_pos);
        kahn_order.push(task_id);

        for &child in &successors[task_id] {
            indegree[child] -= 1;
            if indegree[child] == 0 {
                available.push(child);
            }
        }
    }

    if kahn_order.len() != n {
        let removed = {
            let mut is_removed = vec![false; n];
            for &task_id in &kahn_order {
                is_removed[task_id] = true;
            }
            is_removed
        };
        let blocked = (0..n).filter(|&task_id| !removed[task_id]).collect();
        return Err(Error::Cycle { blocked });
    }

    // Compute bottom levels in reverse topological order.
    let mut tail = vec![0u64; n];
    for &task_id in kahn_order.iter().rev() {
        let max_successor_tail = successors[task_id]
            .iter()
            .map(|&child| tail[child])
            .max()
            .unwrap_or(0);
        tail[task_id] = tasks[task_id]
            .duration
            .checked_add(max_successor_tail)
            .ok_or(Error::Overflow {
                task: task_id,
                stage: Stage::Tail,
            })?;
    }

    let mut runs: Vec<Option<Run>> = vec![None; n];
    let mut events = Vec::new();
    let mut started = vec![false; n];
    let mut completed = vec![false; n];
    let mut remaining_predecessors: Vec<usize> =
        tasks.iter().map(|task| task.deps.len()).collect();

    let mut worker_available_at = vec![0u64; workers];
    let mut running: Vec<Option<(usize, u64)>> = vec![None; workers];
    let mut completed_count = 0usize;
    let mut now = 0u64;
    let mut makespan = 0u64;

    while completed_count < n {
        // Complete all tasks finishing now in ascending task ID.
        let mut completions = Vec::<(usize, usize)>::new();
        for (worker_id, slot) in running.iter().enumerate() {
            if let Some((task_id, finish)) = slot {
                if *finish == now {
                    completions.push((*task_id, worker_id));
                }
            }
        }
        completions.sort_unstable_by_key(|&(task_id, _)| task_id);

        for (task_id, worker_id) in completions {
            running[worker_id] = None;
            completed[task_id] = true;
            completed_count += 1;

            events.push(Event {
                time: now,
                task: task_id,
                worker: worker_id,
                kind: EventKind::Finish,
            });

            for &child in &successors[task_id] {
                remaining_predecessors[child] -= 1;
            }
        }

        // Start tasks one at a time, reevaluating eligibility and ranking each time.
        loop {
            let Some(worker_id) = worker_available_at
                .iter()
                .enumerate()
                .filter(|(worker_id, available_at)| {
                    **available_at <= now && running[*worker_id].is_none()
                })
                .map(|(worker_id, _)| worker_id)
                .min()
            else {
                break;
            };

            let selected = (0..n)
                .filter(|&task_id| {
                    !started[task_id]
                        && tasks[task_id].release <= now
                        && remaining_predecessors[task_id] == 0
                })
                .max_by(|&a, &b| {
                    tail[a]
                        .cmp(&tail[b])
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
            let worker_available = finish.checked_add(turnaround).ok_or(Error::Overflow {
                task: task_id,
                stage: Stage::Turnaround,
            })?;

            started[task_id] = true;
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

            worker_available_at[worker_id] = worker_available;
            running[worker_id] = Some((task_id, finish));

            if finish > makespan {
                makespan = finish;
            }

            if finish == now {
                // Zero-duration tasks complete immediately, before selecting another start.
                running[worker_id] = None;
                completed[task_id] = true;
                completed_count += 1;
                events.push(Event {
                    time: now,
                    task: task_id,
                    worker: worker_id,
                    kind: EventKind::Finish,
                });
                for &child in &successors[task_id] {
                    remaining_predecessors[child] -= 1;
                }
            }
        }

        if completed_count == n {
            break;
        }

        // Advance to the earliest event that can change eligibility or worker availability.
        let mut next_time: Option<u64> = None;
        let mut consider = |time: u64| {
            if time > now && next_time.map_or(true, |current| time < current) {
                next_time = Some(time);
            }
        };

        for slot in &running {
            if let Some((_, finish)) = slot {
                consider(*finish);
            }
        }
        for task_id in 0..n {
            if !started[task_id] && tasks[task_id].release > now {
                consider(tasks[task_id].release);
            }
        }
        for worker_id in 0..workers {
            if running[worker_id].is_none() && worker_available_at[worker_id] > now {
                consider(worker_available_at[worker_id]);
            }
        }

        now = next_time.expect("unfinished tasks must have a future event");
    }

    let runs = runs
        .into_iter()
        .map(|run| run.expect("every task must have started"))
        .collect();

    Ok(Schedule {
        runs,
        makespan,
        events,
    })
}