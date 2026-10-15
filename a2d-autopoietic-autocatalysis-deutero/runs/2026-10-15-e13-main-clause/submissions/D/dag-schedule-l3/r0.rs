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

    // Validate every dependency entry in input order before any other checks.
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

    // Build successor lists and indegrees.
    let mut children = vec![Vec::new(); n];
    let mut indegree = vec![0usize; n];
    for (task_id, task) in tasks.iter().enumerate() {
        indegree[task_id] = task.deps.len();
        for &dep in &task.deps {
            children[dep].push(task_id);
        }
    }

    // Kahn traversal, always removing the smallest available ID.
    let mut available = Vec::new();
    for id in 0..n {
        if indegree[id] == 0 {
            available.push(id);
        }
    }
    let mut order = Vec::with_capacity(n);
    while !available.is_empty() {
        let smallest_pos = available
            .iter()
            .enumerate()
            .min_by_key(|(_, id)| **id)
            .map(|(pos, _)| pos)
            .unwrap();
        let id = available.swap_remove(smallest_pos);
        order.push(id);
        for &child in &children[id] {
            indegree[child] -= 1;
            if indegree[child] == 0 {
                available.push(child);
            }
        }
    }

    if order.len() != n {
        let mut blocked = Vec::new();
        for id in 0..n {
            if indegree[id] != 0 {
                blocked.push(id);
            }
        }
        return Err(Error::Cycle { blocked });
    }

    // Bottom-level tails, computed in reverse topological order.
    let mut tails = vec![0u64; n];
    for &id in order.iter().rev() {
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
    let mut events = Vec::with_capacity(n.saturating_mul(2));
    let mut started = vec![false; n];
    let mut completed = vec![false; n];
    let mut remaining_deps: Vec<usize> = tasks.iter().map(|task| task.deps.len()).collect();

    // Worker availability times and current task assignment.
    let mut worker_available = vec![0u64; workers];
    let mut worker_running: Vec<Option<usize>> = vec![None; workers];

    let mut time = 0u64;
    let mut completed_count = 0usize;
    let mut makespan = 0u64;

    while completed_count < n {
        // Complete all positive-duration tasks finishing now, in task-ID order.
        let mut finishing = Vec::new();
        for worker_id in 0..workers {
            if let Some(task_id) = worker_running[worker_id] {
                if runs[task_id].as_ref().unwrap().finish == time {
                    finishing.push((task_id, worker_id));
                }
            }
        }
        finishing.sort_by_key(|&(task_id, _)| task_id);

        for (task_id, worker_id) in finishing {
            worker_running[worker_id] = None;
            completed[task_id] = true;
            completed_count += 1;
            for &child in &children[task_id] {
                remaining_deps[child] -= 1;
            }
            events.push(Event {
                time,
                task: task_id,
                worker: worker_id,
                kind: EventKind::Finish,
            });
        }

        // Start tasks one at a time, reevaluating after every admission and
        // immediately completing each zero-duration task.
        loop {
            let worker_id = (0..workers).find(|&w| {
                worker_running[w].is_none() && worker_available[w] <= time
            });
            let worker_id = match worker_id {
                Some(w) => w,
                None => break,
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

            let task_id = match selected {
                Some(id) => id,
                None => break,
            };

            let finish = time
                .checked_add(tasks[task_id].duration)
                .ok_or(Error::Overflow {
                    task: task_id,
                    stage: Stage::Finish,
                })?;
            let worker_free = finish
                .checked_add(turnaround)
                .ok_or(Error::Overflow {
                    task: task_id,
                    stage: Stage::Turnaround,
                })?;

            started[task_id] = true;
            worker_available[worker_id] = worker_free;
            worker_running[worker_id] = if finish == time {
                None
            } else {
                Some(task_id)
            };
            runs[task_id] = Some(Run {
                start: time,
                finish,
                worker: worker_id,
            });
            events.push(Event {
                time,
                task: task_id,
                worker: worker_id,
                kind: EventKind::Start,
            });

            if finish == time {
                completed[task_id] = true;
                completed_count += 1;
                for &child in &children[task_id] {
                    remaining_deps[child] -= 1;
                }
                events.push(Event {
                    time,
                    task: task_id,
                    worker: worker_id,
                    kind: EventKind::Finish,
                });
            } else {
                makespan = makespan.max(finish);
            }
        }

        if completed_count == n {
            break;
        }

        // Move to the earliest strictly later event that can change eligibility
        // or worker availability.
        let mut next_time: Option<u64> = None;
        let mut consider = |candidate: u64| {
            if candidate > time {
                next_time = Some(next_time.map_or(candidate, |current| current.min(candidate)));
            }
        };

        for id in 0..n {
            if !started[id] {
                consider(tasks[id].release);
            }
        }
        for worker_id in 0..workers {
            if let Some(task_id) = worker_running[worker_id] {
                consider(runs[task_id].as_ref().unwrap().finish);
            } else if worker_available[worker_id] > time {
                consider(worker_available[worker_id]);
            }
        }

        match next_time {
            Some(next) => time = next,
            None => break,
        }
    }

    let ordered_runs = runs
        .into_iter()
        .map(|run| run.expect("all tasks must start"))
        .collect();

    Ok(Schedule {
        runs: ordered_runs,
        makespan,
        events,
    })
}