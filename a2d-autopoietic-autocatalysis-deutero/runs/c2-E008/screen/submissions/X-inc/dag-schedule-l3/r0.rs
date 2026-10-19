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

    let mut successors = vec![Vec::<usize>::new(); n];
    let mut indegree = vec![0usize; n];
    for (task_id, task) in tasks.iter().enumerate() {
        indegree[task_id] = task.deps.len();
        for &predecessor in &task.deps {
            successors[predecessor].push(task_id);
        }
    }

    // Kahn order uses the smallest currently available task ID.
    let mut available = std::collections::BinaryHeap::new();
    for (task_id, &degree) in indegree.iter().enumerate() {
        if degree == 0 {
            available.push(std::cmp::Reverse(task_id));
        }
    }

    let mut order = Vec::with_capacity(n);
    let mut remaining_indegree = indegree.clone();
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
        let removed: Vec<bool> = {
            let mut flags = vec![false; n];
            for &task_id in &order {
                flags[task_id] = true;
            }
            flags
        };
        return Err(Error::Cycle {
            blocked: (0..n).filter(|&task_id| !removed[task_id]).collect(),
        });
    }

    let mut tail = vec![0u64; n];
    for &task_id in order.iter().rev() {
        let max_child_tail = successors[task_id]
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

    #[derive(Clone, Copy)]
    struct Running {
        task: usize,
        worker: usize,
        finish: u64,
    }

    let mut runs: Vec<Option<Run>> = vec![None; n];
    let mut events = Vec::new();
    let mut started = vec![false; n];
    let mut completed = vec![false; n];
    let mut remaining_predecessors = indegree;
    let mut worker_available = vec![0u64; workers];
    let mut running: Vec<Running> = Vec::new();
    let mut time = 0u64;
    let mut completed_count = 0usize;
    let mut makespan = 0u64;

    while completed_count < n {
        // Complete all tasks finishing now, in ascending task ID.
        let mut finished_now: Vec<Running> = running
            .iter()
            .copied()
            .filter(|entry| entry.finish == time)
            .collect();
        finished_now.sort_by_key(|entry| entry.task);

        if !finished_now.is_empty() {
            running.retain(|entry| entry.finish != time);
            for entry in finished_now {
                completed[entry.task] = true;
                completed_count += 1;
                makespan = makespan.max(time);
                events.push(Event {
                    time,
                    task: entry.task,
                    worker: entry.worker,
                    kind: EventKind::Finish,
                });
                for &child in &successors[entry.task] {
                    remaining_predecessors[child] -= 1;
                }
            }
        }

        // Admit one task at a time, reevaluating eligibility and rank each time.
        loop {
            let worker = worker_available
                .iter()
                .position(|&available_at| available_at <= time);
            let Some(worker) = worker else {
                break;
            };

            let selected = (0..n)
                .filter(|&task_id| {
                    !started[task_id]
                        && tasks[task_id].release <= time
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

            let finish = time.checked_add(tasks[task_id].duration).ok_or(Error::Overflow {
                task: task_id,
                stage: Stage::Finish,
            })?;
            let worker_ready = finish.checked_add(turnaround).ok_or(Error::Overflow {
                task: task_id,
                stage: Stage::Turnaround,
            })?;

            started[task_id] = true;
            worker_available[worker] = worker_ready;
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
                // Zero-duration tasks complete before choosing another start.
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
                    remaining_predecessors[child] -= 1;
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

        // Advance to the next relevant event strictly after the current time.
        let next_finish = running.iter().map(|entry| entry.finish).min();
        let next_release = (0..n)
            .filter(|&task_id| !started[task_id] && tasks[task_id].release > time)
            .map(|task_id| tasks[task_id].release)
            .min();
        let next_worker = worker_available
            .iter()
            .copied()
            .filter(|&available_at| available_at > time)
            .min();

        let next_time = [next_finish, next_release, next_worker]
            .into_iter()
            .flatten()
            .min();

        match next_time {
            Some(next_time) => time = next_time,
            None => {
                // A valid acyclic graph always has a future event if unfinished work remains.
                // This branch avoids panicking even if internal invariants are violated.
                return Err(Error::Cycle {
                    blocked: (0..n).filter(|&task_id| !completed[task_id]).collect(),
                });
            }
        }
    }

    Ok(Schedule {
        runs: runs
            .into_iter()
            .map(|run| run.expect("every task starts exactly once"))
            .collect(),
        makespan,
        events,
    })
}