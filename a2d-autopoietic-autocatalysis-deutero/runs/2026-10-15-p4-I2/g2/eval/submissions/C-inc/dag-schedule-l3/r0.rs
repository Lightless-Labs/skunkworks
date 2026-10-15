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

    let mut available = Vec::new();
    for id in 0..n {
        if indegree[id] == 0 {
            available.push(id);
        }
    }

    let mut topo = Vec::with_capacity(n);
    while !available.is_empty() {
        let smallest_index = available
            .iter()
            .enumerate()
            .min_by_key(|(_, id)| **id)
            .map(|(index, _)| index)
            .unwrap();
        let id = available.swap_remove(smallest_index);
        topo.push(id);

        for &child in &children[id] {
            indegree[child] -= 1;
            if indegree[child] == 0 {
                available.push(child);
            }
        }
    }

    if topo.len() != n {
        let removed = {
            let mut flags = vec![false; n];
            for &id in &topo {
                flags[id] = true;
            }
            flags
        };
        let blocked = (0..n).filter(|&id| !removed[id]).collect();
        return Err(Error::Cycle { blocked });
    }

    let mut tails = vec![0u64; n];
    for &id in topo.iter().rev() {
        let max_successor_tail = children[id]
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
    let mut started = vec![false; n];
    let mut completed = vec![false; n];
    let mut remaining_deps: Vec<usize> = tasks.iter().map(|task| task.deps.len()).collect();
    let mut worker_available = vec![0u64; workers];
    let mut running: Vec<(u64, usize, usize)> = Vec::new();
    let mut events = Vec::with_capacity(n.saturating_mul(2));
    let mut time = 0u64;
    let mut finished_count = 0usize;
    let mut makespan = 0u64;

    while finished_count < n {
        let mut completed_now: Vec<(u64, usize, usize)> = running
            .iter()
            .copied()
            .filter(|(finish, _, _)| *finish == time)
            .collect();
        completed_now.sort_by_key(|(_, task, _)| *task);

        for &(_, task, worker) in &completed_now {
            completed[task] = true;
            finished_count += 1;
            makespan = makespan.max(time);
            events.push(Event {
                time,
                task,
                worker,
                kind: EventKind::Finish,
            });
        }

        if !completed_now.is_empty() {
            running.retain(|(finish, _, _)| *finish != time);
            for &(_, task, _) in &completed_now {
                for &child in &children[task] {
                    remaining_deps[child] -= 1;
                }
            }
        }

        loop {
            let worker = worker_available
                .iter()
                .enumerate()
                .filter(|(_, available_at)| **available_at <= time)
                .map(|(id, _)| id)
                .min();
            let Some(worker) = worker else {
                break;
            };

            let selected = (0..n)
                .filter(|&id| {
                    !started[id]
                        && tasks[id].release <= time
                        && remaining_deps[id] == 0
                })
                .max_by(|&a, &b| {
                    tails[a]
                        .cmp(&tails[b])
                        .then_with(|| tasks[a].priority.cmp(&tasks[b].priority))
                        .then_with(|| b.cmp(&a))
                });
            let Some(task) = selected else {
                break;
            };

            let finish = time
                .checked_add(tasks[task].duration)
                .ok_or(Error::Overflow {
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
                start: time,
                finish,
                worker,
            });
            events.push(Event {
                time,
                task,
                worker,
                kind: EventKind::Start,
            });

            if finish == time {
                completed[task] = true;
                finished_count += 1;
                events.push(Event {
                    time,
                    task,
                    worker,
                    kind: EventKind::Finish,
                });
                makespan = makespan.max(time);
                for &child in &children[task] {
                    remaining_deps[child] -= 1;
                }
            } else {
                running.push((finish, task, worker));
            }
        }

        if finished_count == n {
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
        for id in 0..n {
            if !started[id] {
                consider(tasks[id].release);
            }
        }
        for &available_at in &worker_available {
            consider(available_at);
        }

        time = next_time.expect("unfinished tasks must have a future event");
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