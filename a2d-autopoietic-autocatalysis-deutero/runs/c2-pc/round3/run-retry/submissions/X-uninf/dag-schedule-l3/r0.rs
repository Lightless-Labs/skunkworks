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

    // Validate dependency entries in the specified order.
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
    let mut successors = vec![Vec::new(); n];
    let mut indegree = vec![0usize; n];
    for (task_id, task) in tasks.iter().enumerate() {
        indegree[task_id] = task.deps.len();
        for &pred in &task.deps {
            successors[pred].push(task_id);
        }
    }

    // Kahn traversal, always removing the smallest currently available ID.
    let mut available = Vec::new();
    for (id, &degree) in indegree.iter().enumerate() {
        if degree == 0 {
            available.push(id);
        }
    }
    let mut topo = Vec::with_capacity(n);
    while !available.is_empty() {
        let smallest_pos = available
            .iter()
            .enumerate()
            .min_by_key(|(_, id)| **id)
            .map(|(pos, _)| pos)
            .unwrap();
        let id = available.swap_remove(smallest_pos);
        topo.push(id);
        for &child in &successors[id] {
            indegree[child] -= 1;
            if indegree[child] == 0 {
                available.push(child);
            }
        }
    }

    if topo.len() != n {
        let removed = {
            let mut done = vec![false; n];
            for &id in &topo {
                done[id] = true;
            }
            done
        };
        return Err(Error::Cycle {
            blocked: (0..n).filter(|&id| !removed[id]).collect(),
        });
    }

    // Compute bottom levels in reverse Kahn order.
    let mut tails = vec![0u64; n];
    for &id in topo.iter().rev() {
        let max_child_tail = successors[id]
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

    #[derive(Clone, Copy)]
    struct Running {
        task: usize,
        worker: usize,
        finish: u64,
    }

    let mut running: Vec<Running> = Vec::new();
    let mut worker_available_at = vec![0u64; workers];
    let mut time = 0u64;
    let mut completed_count = 0usize;
    let mut makespan = 0u64;

    while completed_count < n {
        // Complete all tasks finishing now in task ID order.
        let mut finishing: Vec<Running> = running
            .iter()
            .copied()
            .filter(|item| item.finish == time)
            .collect();
        finishing.sort_by_key(|item| item.task);

        if !finishing.is_empty() {
            let finishing_ids: Vec<usize> = finishing.iter().map(|item| item.task).collect();
            running.retain(|item| !finishing_ids.contains(&item.task));

            for item in finishing {
                completed[item.task] = true;
                completed_count += 1;
                makespan = makespan.max(item.finish);
                events.push(Event {
                    time,
                    task: item.task,
                    worker: item.worker,
                    kind: EventKind::Finish,
                });
                for &child in &successors[item.task] {
                    remaining_predecessors[child] -= 1;
                }
            }
        }

        loop {
            let worker = (0..workers).find(|&id| worker_available_at[id] <= time);
            let Some(worker) = worker else {
                break;
            };

            let selected = (0..n)
                .filter(|&id| {
                    !started[id]
                        && tasks[id].release <= time
                        && remaining_predecessors[id] == 0
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

            let finish = time
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

            if finish == time {
                // Zero-duration tasks complete immediately before another admission.
                completed[task_id] = true;
                completed_count += 1;
                makespan = makespan.max(finish);
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

        let mut next_time: Option<u64> = None;
        let mut consider = |candidate: u64| {
            if candidate > time {
                next_time = Some(next_time.map_or(candidate, |current| current.min(candidate)));
            }
        };

        for item in &running {
            consider(item.finish);
        }
        for id in 0..n {
            if !started[id] && remaining_predecessors[id] == 0 {
                consider(tasks[id].release);
            }
        }
        for &available_at in &worker_available_at {
            consider(available_at);
        }

        // A task may become startable when its last predecessor completes; such a
        // completion is already represented by a running task's finish event.
        time = next_time.expect("unfinished tasks must have a future event");
    }

    let runs = runs.into_iter().map(Option::unwrap).collect();
    Ok(Schedule {
        runs,
        makespan,
        events,
    })
}