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
    BadDependency { task: usize, position: usize, value: usize },
    SelfDependency { task: usize, position: usize },
    DuplicateDependency { task: usize, position: usize, value: usize },
    Cycle { blocked: Vec<usize> },
    Workers { count: usize },
    Overflow { task: usize, stage: Stage },
}

pub fn schedule(tasks: &[Task], workers: usize, turnaround: u64) -> Result<Schedule, Error> {
    let n = tasks.len();

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
    let mut children = vec![Vec::new(); n];
    for (task_id, task) in tasks.iter().enumerate() {
        for &dependency in &task.deps {
            children[dependency].push(task_id);
        }
    }

    let mut available: Vec<usize> = (0..n).filter(|&id| indegree[id] == 0).collect();
    let mut order = Vec::with_capacity(n);
    while !available.is_empty() {
        let smallest_index = available
            .iter()
            .enumerate()
            .min_by_key(|(_, id)| **id)
            .map(|(index, _)| index)
            .unwrap();
        let task_id = available.swap_remove(smallest_index);
        order.push(task_id);
        for &child in &children[task_id] {
            indegree[child] -= 1;
            if indegree[child] == 0 {
                available.push(child);
            }
        }
    }

    if order.len() != n {
        let mut removed = vec![false; n];
        for &id in &order {
            removed[id] = true;
        }
        return Err(Error::Cycle {
            blocked: (0..n).filter(|&id| !removed[id]).collect(),
        });
    }

    let mut tails = vec![0u64; n];
    for &task_id in order.iter().rev() {
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
    let mut events = Vec::with_capacity(n.saturating_mul(2));
    let mut completed = vec![false; n];
    let mut started = vec![false; n];
    let mut worker_available = vec![0u64; workers];
    let mut running: Vec<(u64, usize, usize)> = Vec::new();
    let mut current_time = 0u64;
    let mut completed_count = 0usize;
    let mut makespan = 0u64;

    while completed_count < n {
        let mut finishing: Vec<(u64, usize, usize)> = running
            .iter()
            .copied()
            .filter(|&(finish, _, _)| finish == current_time)
            .collect();
        finishing.sort_by_key(|&(_, task_id, _)| task_id);

        for &(finish, task_id, worker_id) in &finishing {
            events.push(Event {
                time: finish,
                task: task_id,
                worker: worker_id,
                kind: EventKind::Finish,
            });
            completed[task_id] = true;
            completed_count += 1;
            makespan = makespan.max(finish);
        }
        running.retain(|&(finish, _, _)| finish != current_time);

        loop {
            let worker_id = (0..workers).find(|&id| worker_available[id] <= current_time);
            let Some(worker_id) = worker_id else {
                break;
            };

            let selected = (0..n)
                .filter(|&id| {
                    !started[id]
                        && tasks[id].release <= current_time
                        && tasks[id].deps.iter().all(|&dep| completed[dep])
                })
                .max_by(|&a, &b| {
                    tails[a]
                        .cmp(&tails[b])
                        .then_with(|| tasks[a].priority.cmp(&tasks[b].priority))
                        .then_with(|| b.cmp(&a))
                });

            let Some(task_id) = selected else {
                break;
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

            if finish == current_time {
                events.push(Event {
                    time: finish,
                    task: task_id,
                    worker: worker_id,
                    kind: EventKind::Finish,
                });
                completed[task_id] = true;
                completed_count += 1;
                makespan = makespan.max(finish);
            } else {
                running.push((finish, task_id, worker_id));
            }
        }

        if completed_count == n {
            break;
        }

        let next_finish = running.iter().map(|&(finish, _, _)| finish).min();
        let next_release = (0..n)
            .filter(|&id| !started[id])
            .map(|id| tasks[id].release)
            .filter(|&release| release > current_time)
            .min();
        let next_worker = worker_available
            .iter()
            .copied()
            .filter(|&time| time > current_time)
            .min();

        let next_time = [next_finish, next_release, next_worker]
            .into_iter()
            .flatten()
            .min();

        match next_time {
            Some(time) => current_time = time,
            None => break,
        }
    }

    Ok(Schedule {
        runs: runs.into_iter().map(Option::unwrap).collect(),
        makespan,
        events,
    })
}