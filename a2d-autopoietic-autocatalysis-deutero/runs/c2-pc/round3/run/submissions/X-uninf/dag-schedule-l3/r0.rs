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

    let mut indegree: Vec<usize> = tasks.iter().map(|task| task.deps.len()).collect();
    let mut children = vec![Vec::<usize>::new(); n];
    for (task_id, task) in tasks.iter().enumerate() {
        for &predecessor in &task.deps {
            children[predecessor].push(task_id);
        }
    }

    // Kahn traversal, always removing the smallest available task ID.
    let mut available = Vec::<usize>::new();
    for (id, &degree) in indegree.iter().enumerate() {
        if degree == 0 {
            available.push(id);
        }
    }
    let mut kahn_order = Vec::with_capacity(n);
    while !available.is_empty() {
        let smallest_pos = available
            .iter()
            .enumerate()
            .min_by_key(|(_, id)| **id)
            .map(|(pos, _)| pos)
            .unwrap();
        let id = available.swap_remove(smallest_pos);
        kahn_order.push(id);

        for &child in &children[id] {
            indegree[child] -= 1;
            if indegree[child] == 0 {
                available.push(child);
            }
        }
    }

    if kahn_order.len() != n {
        let mut removed = vec![false; n];
        for &id in &kahn_order {
            removed[id] = true;
        }
        return Err(Error::Cycle {
            blocked: (0..n).filter(|&id| !removed[id]).collect(),
        });
    }

    // Compute bottom levels in reverse Kahn order.
    let mut tails = vec![0u64; n];
    for &id in kahn_order.iter().rev() {
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
    let mut events = Vec::new();
    let mut started = vec![false; n];
    let mut completed = vec![false; n];
    let mut remaining_predecessors: Vec<usize> =
        tasks.iter().map(|task| task.deps.len()).collect();

    let mut worker_available = vec![0u64; workers];
    let mut running = Vec::<(u64, usize, usize)>::new(); // (finish, task, worker)
    let mut completed_count = 0usize;
    let mut now = 0u64;
    let mut makespan = 0u64;

    while completed_count < n {
        // Complete all tasks finishing now, sorted by task ID.
        let mut due: Vec<(u64, usize, usize)> = running
            .iter()
            .copied()
            .filter(|(finish, _, _)| *finish == now)
            .collect();
        due.sort_by_key(|(_, task, _)| *task);

        if !due.is_empty() {
            running.retain(|(finish, _, _)| *finish != now);
            for (finish, task_id, worker_id) in due {
                completed[task_id] = true;
                completed_count += 1;
                makespan = makespan.max(finish);
                events.push(Event {
                    time: finish,
                    task: task_id,
                    worker: worker_id,
                    kind: EventKind::Finish,
                });
                for &child in &children[task_id] {
                    remaining_predecessors[child] -= 1;
                }
            }
        }

        // Repeatedly start one eligible task, then reconsider priorities.
        loop {
            let worker_id = match worker_available
                .iter()
                .enumerate()
                .filter(|(_, available_at)| **available_at <= now)
                .map(|(id, _)| id)
                .min()
            {
                Some(id) => id,
                None => break,
            };

            let selected = (0..n)
                .filter(|&id| {
                    !started[id]
                        && tasks[id].release <= now
                        && remaining_predecessors[id] == 0
                })
                .max_by(|&a, &b| {
                    tails[a]
                        .cmp(&tails[b])
                        .then_with(|| tasks[a].priority.cmp(&tasks[b].priority))
                        .then_with(|| b.cmp(&a))
                });

            let task_id = match selected {
                Some(id) => id,
                None => break,
            };

            let finish = now
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

            if tasks[task_id].duration == 0 {
                // A zero-duration task completes immediately before another start.
                completed[task_id] = true;
                completed_count += 1;
                makespan = makespan.max(finish);
                events.push(Event {
                    time: finish,
                    task: task_id,
                    worker: worker_id,
                    kind: EventKind::Finish,
                });
                for &child in &children[task_id] {
                    remaining_predecessors[child] -= 1;
                }
            } else {
                running.push((finish, task_id, worker_id));
            }
        }

        if completed_count == n {
            break;
        }

        // Advance to the earliest strictly later event that may change eligibility.
        let mut next_time: Option<u64> = None;
        for &(finish, _, _) in &running {
            if finish > now {
                next_time = Some(next_time.map_or(finish, |current| current.min(finish)));
            }
        }
        for id in 0..n {
            if !started[id] && tasks[id].release > now {
                let release = tasks[id].release;
                next_time = Some(next_time.map_or(release, |current| current.min(release)));
            }
        }
        for &available_at in &worker_available {
            if available_at > now {
                next_time = Some(next_time.map_or(available_at, |current| current.min(available_at)));
            }
        }

        // In a valid acyclic graph, an unstarted task can only be blocked by a
        // running predecessor, a release, or worker cooldown.
        now = next_time.expect("unfinished tasks must have a future scheduling event");
    }

    let runs = runs
        .into_iter()
        .map(|run| run.expect("every task starts exactly once"))
        .collect();

    Ok(Schedule {
        runs,
        makespan,
        events,
    })
}