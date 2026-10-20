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

use std::cmp::Reverse;
use std::collections::BinaryHeap;

pub fn schedule(tasks: &[Task], workers: usize, turnaround: u64) -> Result<Schedule, Error> {
    let n = tasks.len();

    // Validate every dependency before any other validation.
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

    let mut indegree: Vec<usize> = tasks.iter().map(|t| t.deps.len()).collect();
    let mut children = vec![Vec::new(); n];
    for (task_id, task) in tasks.iter().enumerate() {
        for &pred in &task.deps {
            children[pred].push(task_id);
        }
    }

    let mut ready = BinaryHeap::new();
    for (id, &degree) in indegree.iter().enumerate() {
        if degree == 0 {
            ready.push(Reverse(id));
        }
    }

    let mut topo = Vec::with_capacity(n);
    while let Some(Reverse(id)) = ready.pop() {
        topo.push(id);
        for &child in &children[id] {
            indegree[child] -= 1;
            if indegree[child] == 0 {
                ready.push(Reverse(child));
            }
        }
    }

    if topo.len() != n {
        let removed: Vec<bool> = {
            let mut removed = vec![false; n];
            for &id in &topo {
                removed[id] = true;
            }
            removed
        };
        return Err(Error::Cycle {
            blocked: (0..n).filter(|&id| !removed[id]).collect(),
        });
    }

    let mut tails = vec![0u64; n];
    for &id in topo.iter().rev() {
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
    let mut started = vec![false; n];
    let mut completed = vec![false; n];
    let mut remaining_preds: Vec<usize> = tasks.iter().map(|t| t.deps.len()).collect();

    // Available workers have reached their cooldown expiry.
    let mut worker_available = vec![0u64; workers];

    // Running completions are ordered by (finish time, task ID, worker ID).
    let mut running: BinaryHeap<Reverse<(u64, usize, usize)>> = BinaryHeap::new();

    let mut events = Vec::with_capacity(n.saturating_mul(2));
    let mut now = 0u64;
    let mut completion_count = 0usize;
    let mut makespan = 0u64;

    loop {
        // Complete all tasks due now, in ascending task ID.
        while let Some(Reverse((finish, task_id, worker_id))) = running.peek().copied() {
            if finish != now {
                break;
            }
            running.pop();
            completed[task_id] = true;
            completion_count += 1;
            makespan = makespan.max(finish);
            events.push(Event {
                time: finish,
                task: task_id,
                worker: worker_id,
                kind: EventKind::Finish,
            });
            for &child in &children[task_id] {
                remaining_preds[child] -= 1;
            }
        }

        if completion_count == n {
            break;
        }

        // Repeatedly select and start one eligible task.
        loop {
            let worker_id = (0..workers).find(|&w| worker_available[w] <= now);
            let Some(worker_id) = worker_id else {
                break;
            };

            let mut best: Option<usize> = None;
            for id in 0..n {
                if started[id]
                    || tasks[id].release > now
                    || remaining_preds[id] != 0
                {
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
                completed[task_id] = true;
                completion_count += 1;
                makespan = makespan.max(finish);
                events.push(Event {
                    time: finish,
                    task: task_id,
                    worker: worker_id,
                    kind: EventKind::Finish,
                });
                for &child in &children[task_id] {
                    remaining_preds[child] -= 1;
                }
            } else {
                running.push(Reverse((finish, task_id, worker_id)));
            }
        }

        if completion_count == n {
            break;
        }

        // Advance to the earliest event that can change eligibility or worker availability.
        let mut next = running.peek().map(|Reverse((time, _, _))| *time);
        for (id, task) in tasks.iter().enumerate() {
            if !started[id] && task.release > now {
                next = Some(next.map_or(task.release, |t| t.min(task.release)));
            }
        }
        for &available_at in &worker_available {
            if available_at > now {
                next = Some(next.map_or(available_at, |t| t.min(available_at)));
            }
        }

        // A valid DAG with unfinished tasks always has a future release,
        // completion, or cooldown event.
        now = next.expect("unfinished valid schedule must have a future event");
    }

    let runs = runs
        .into_iter()
        .map(|run| run.expect("every task must start"))
        .collect();

    Ok(Schedule {
        runs,
        makespan,
        events,
    })
}