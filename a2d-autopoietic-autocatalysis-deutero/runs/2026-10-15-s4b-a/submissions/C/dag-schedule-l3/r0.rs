use std::cmp::Reverse;
use std::collections::{BTreeSet, BinaryHeap};

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

    // Dependency validation precedes all other validation.
    for (task_id, task) in tasks.iter().enumerate() {
        let mut seen = BTreeSet::new();
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
            if !seen.insert(value) {
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

    // Build predecessor counts and successor lists.
    let mut remaining_deps = vec![0usize; n];
    let mut children = vec![Vec::new(); n];
    for (task_id, task) in tasks.iter().enumerate() {
        remaining_deps[task_id] = task.deps.len();
        for &dep in &task.deps {
            children[dep].push(task_id);
        }
    }

    // Kahn order always removes the smallest available ID.
    let mut available = BTreeSet::new();
    for id in 0..n {
        if remaining_deps[id] == 0 {
            available.insert(id);
        }
    }
    let mut order = Vec::with_capacity(n);
    while let Some(&id) = available.iter().next() {
        available.remove(&id);
        order.push(id);
        for &child in &children[id] {
            remaining_deps[child] -= 1;
            if remaining_deps[child] == 0 {
                available.insert(child);
            }
        }
    }

    if order.len() != n {
        let removed: BTreeSet<usize> = order.into_iter().collect();
        let blocked = (0..n).filter(|id| !removed.contains(id)).collect();
        return Err(Error::Cycle { blocked });
    }

    // Compute bottom levels in reverse Kahn order.
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
    let mut completed = vec![false; n];
    let mut started = vec![false; n];
    let mut worker_available = vec![0u64; workers];

    // Running entries are (finish time, task ID, worker ID).
    let mut running: BinaryHeap<Reverse<(u64, usize, usize)>> = BinaryHeap::new();
    let mut completed_count = 0usize;
    let mut time = 0u64;
    let mut makespan = 0u64;

    while completed_count < n {
        // Complete every task finishing now in ascending task ID.
        let mut finished_now = Vec::new();
        while let Some(Reverse((finish, task_id, worker_id))) = running.peek().copied() {
            if finish != time {
                break;
            }
            running.pop();
            finished_now.push((task_id, worker_id));
        }
        finished_now.sort_unstable_by_key(|&(task_id, _)| task_id);
        for (task_id, worker_id) in finished_now {
            completed[task_id] = true;
            completed_count += 1;
            events.push(Event {
                time,
                task: task_id,
                worker: worker_id,
                kind: EventKind::Finish,
            });
        }

        // Admit one task at a time, reevaluating eligibility and ranking each time.
        loop {
            let worker_id = match worker_available.iter().position(|&available_at| available_at <= time) {
                Some(id) => id,
                None => break,
            };

            let mut best: Option<usize> = None;
            for task_id in 0..n {
                if started[task_id] || tasks[task_id].release > time {
                    continue;
                }
                if tasks[task_id].deps.iter().any(|&dep| !completed[dep]) {
                    continue;
                }

                best = match best {
                    None => Some(task_id),
                    Some(old) => {
                        let rank_new = (tails[task_id], tasks[task_id].priority);
                        let rank_old = (tails[old], tasks[old].priority);
                        if rank_new > rank_old || (rank_new == rank_old && task_id < old) {
                            Some(task_id)
                        } else {
                            Some(old)
                        }
                    }
                };
            }

            let task_id = match best {
                Some(id) => id,
                None => break,
            };

            let finish = time.checked_add(tasks[task_id].duration).ok_or(Error::Overflow {
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
            makespan = makespan.max(finish);

            if finish == time {
                // A zero-duration task completes immediately before the next admission.
                completed[task_id] = true;
                completed_count += 1;
                events.push(Event {
                    time,
                    task: task_id,
                    worker: worker_id,
                    kind: EventKind::Finish,
                });
            } else {
                running.push(Reverse((finish, task_id, worker_id)));
            }
        }

        if completed_count == n {
            break;
        }

        // Find the earliest strictly later event that can change eligibility.
        let mut next_time: Option<u64> = None;
        if let Some(Reverse((finish, _, _))) = running.peek().copied() {
            if finish > time {
                next_time = Some(finish);
            }
        }
        for (task_id, task) in tasks.iter().enumerate() {
            if !started[task_id] && task.release > time {
                next_time = Some(next_time.map_or(task.release, |current| current.min(task.release)));
            }
        }
        for &available_at in &worker_available {
            if available_at > time {
                next_time = Some(next_time.map_or(available_at, |current| current.min(available_at)));
            }
        }

        match next_time {
            Some(next) => time = next,
            None => break,
        }
    }

    Ok(Schedule {
        runs: runs.into_iter().map(Option::unwrap).collect(),
        makespan,
        events,
    })
}

fn main() {}