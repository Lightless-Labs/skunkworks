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

    // Validate every dependency entry before other validation.
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

    let mut indegree = vec![0usize; n];
    let mut children = vec![Vec::new(); n];
    for (task_id, task) in tasks.iter().enumerate() {
        indegree[task_id] = task.deps.len();
        for &dep in &task.deps {
            children[dep].push(task_id);
        }
    }

    // Kahn traversal with the smallest available task ID selected each time.
    let mut available: Vec<usize> = (0..n).filter(|&id| indegree[id] == 0).collect();
    let mut order = Vec::with_capacity(n);
    while !available.is_empty() {
        let smallest_index = available
            .iter()
            .enumerate()
            .min_by_key(|&(_, id)| *id)
            .map(|(index, _)| index)
            .unwrap();
        let id = available.swap_remove(smallest_index);
        order.push(id);

        for &child in &children[id] {
            indegree[child] -= 1;
            if indegree[child] == 0 {
                available.push(child);
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

    let mut tails = vec![0u64; n];
    for &id in order.iter().rev() {
        let successor_tail = children[id]
            .iter()
            .map(|&child| tails[child])
            .max()
            .unwrap_or(0);
        tails[id] = tasks[id]
            .duration
            .checked_add(successor_tail)
            .ok_or(Error::Overflow {
                task: id,
                stage: Stage::Tail,
            })?;
    }

    let mut remaining_predecessors: Vec<usize> =
        tasks.iter().map(|task| task.deps.len()).collect();
    let mut started = vec![false; n];
    let mut completed = vec![false; n];
    let mut runs: Vec<Option<Run>> = vec![None; n];
    let mut worker_available = vec![0u64; workers];
    let mut running: Vec<(u64, usize, usize)> = Vec::new(); // (finish, task, worker)
    let mut events = Vec::new();
    let mut current_time = 0u64;
    let mut completed_count = 0usize;
    let mut makespan = 0u64;

    while completed_count < n {
        // Complete all tasks finishing now, sorted by task ID.
        let mut finishing: Vec<(u64, usize, usize)> = Vec::new();
        let mut still_running = Vec::with_capacity(running.len());
        for entry in running.drain(..) {
            if entry.0 == current_time {
                finishing.push(entry);
            } else {
                still_running.push(entry);
            }
        }
        running = still_running;
        finishing.sort_by_key(|entry| entry.1);

        for &(finish, task_id, worker_id) in &finishing {
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

        loop {
            let worker_id = match worker_available
                .iter()
                .enumerate()
                .filter(|&(_, &available_at)| available_at <= current_time)
                .map(|(id, _)| id)
                .min()
            {
                Some(id) => id,
                None => break,
            };

            let selected = (0..n)
                .filter(|&id| {
                    !started[id]
                        && tasks[id].release <= current_time
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

            let finish = current_time
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
                start: current_time,
                finish,
                worker: worker_id,
            });
            events.push(Event {
                time: current_time,
                task: task_id,
                worker: worker_id,
                kind: EventKind::Start,
            });

            if tasks[task_id].duration == 0 {
                // Immediate completion occurs before choosing the next start.
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

        let mut next_time: Option<u64> = None;
        let mut consider = |time: u64| {
            if time > current_time {
                next_time = Some(next_time.map_or(time, |old| old.min(time)));
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
        for &time in &worker_available {
            consider(time);
        }

        current_time = next_time.expect("unfinished tasks must have a future event");
    }

    let runs = runs.into_iter().map(Option::unwrap).collect();
    Ok(Schedule {
        runs,
        makespan,
        events,
    })
}

fn main() {}