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

    // Validate every dependency before checking workers or doing any computation.
    for (task_id, task) in tasks.iter().enumerate() {
        for (position, &dep) in task.deps.iter().enumerate() {
            if dep >= n {
                return Err(Error::BadDependency {
                    task: task_id,
                    position,
                    value: dep,
                });
            }
            if dep == task_id {
                return Err(Error::SelfDependency {
                    task: task_id,
                    position,
                });
            }
            if task.deps[..position].contains(&dep) {
                return Err(Error::DuplicateDependency {
                    task: task_id,
                    position,
                    value: dep,
                });
            }
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

    // Kahn traversal always removes the smallest currently available ID.
    let mut available = std::collections::BinaryHeap::new();
    for task_id in 0..n {
        if indegree[task_id] == 0 {
            available.push(std::cmp::Reverse(task_id));
        }
    }
    let mut topo = Vec::with_capacity(n);
    let mut remaining_indegree = indegree.clone();
    while let Some(std::cmp::Reverse(task_id)) = available.pop() {
        topo.push(task_id);
        for &child in &children[task_id] {
            remaining_indegree[child] -= 1;
            if remaining_indegree[child] == 0 {
                available.push(std::cmp::Reverse(child));
            }
        }
    }

    if topo.len() != n {
        let removed = {
            let mut set = vec![false; n];
            for &task_id in &topo {
                set[task_id] = true;
            }
            set
        };
        let blocked = (0..n).filter(|&task_id| !removed[task_id]).collect();
        return Err(Error::Cycle { blocked });
    }

    // Compute bottom levels in reverse Kahn order.
    let mut tails = vec![0u64; n];
    for &task_id in topo.iter().rev() {
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
    let mut started = vec![false; n];
    let mut completed = vec![false; n];
    let mut remaining_deps = indegree;
    let mut worker_available = vec![0u64; workers];
    let mut running: Vec<(u64, usize, usize)> = Vec::new(); // (finish, task, worker)
    let mut events = Vec::new();
    let mut now = 0u64;
    let mut completed_count = 0usize;
    let mut makespan = 0u64;

    while completed_count < n {
        // Complete all currently running tasks at this time, in task-ID order.
        let mut finishing: Vec<(u64, usize, usize)> =
            running.iter().copied().filter(|&(finish, _, _)| finish == now).collect();
        finishing.sort_by_key(|&(_, task_id, _)| task_id);
        if !finishing.is_empty() {
            running.retain(|&(finish, _, _)| finish != now);
            for &(_, task_id, worker_id) in &finishing {
                completed[task_id] = true;
                completed_count += 1;
                makespan = makespan.max(now);
                events.push(Event {
                    time: now,
                    task: task_id,
                    worker: worker_id,
                    kind: EventKind::Finish,
                });
                for &child in &children[task_id] {
                    remaining_deps[child] -= 1;
                }
            }
        }

        // Keep starting tasks while a worker and eligible task are available.
        loop {
            let worker_id = match worker_available.iter().position(|&t| t <= now) {
                Some(id) => id,
                None => break,
            };

            let mut eligible: Vec<usize> = (0..n)
                .filter(|&task_id| {
                    !started[task_id]
                        && tasks[task_id].release <= now
                        && remaining_deps[task_id] == 0
                })
                .collect();
            if eligible.is_empty() {
                break;
            }
            eligible.sort_by(|&a, &b| {
                tails[b]
                    .cmp(&tails[a])
                    .then_with(|| tasks[b].priority.cmp(&tasks[a].priority))
                    .then_with(|| a.cmp(&b))
            });
            let task_id = eligible[0];

            let finish = now
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

            if finish == now {
                // Immediate completion of a zero-duration task.
                completed[task_id] = true;
                completed_count += 1;
                makespan = makespan.max(now);
                events.push(Event {
                    time: now,
                    task: task_id,
                    worker: worker_id,
                    kind: EventKind::Finish,
                });
                for &child in &children[task_id] {
                    remaining_deps[child] -= 1;
                }
            } else {
                running.push((finish, task_id, worker_id));
            }
        }

        if completed_count == n {
            break;
        }

        // Advance to the earliest future event that can change eligibility or capacity.
        let mut next_time: Option<u64> = None;
        for &(finish, _, _) in &running {
            if finish > now {
                next_time = Some(next_time.map_or(finish, |t| t.min(finish)));
            }
        }
        for task_id in 0..n {
            if !started[task_id] && tasks[task_id].release > now {
                let release = tasks[task_id].release;
                next_time = Some(next_time.map_or(release, |t| t.min(release)));
            }
        }
        for &available_at in &worker_available {
            if available_at > now {
                next_time = Some(next_time.map_or(available_at, |t| t.min(available_at)));
            }
        }

        match next_time {
            Some(t) => now = t,
            None => break,
        }
    }

    let runs = runs
        .into_iter()
        .map(|run| run.expect("all tasks are started when scheduling succeeds"))
        .collect();

    Ok(Schedule {
        runs,
        makespan,
        events,
    })
}

fn main() {}