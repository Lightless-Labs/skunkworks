use std::cmp::Reverse;
use std::collections::BinaryHeap;
use std::fmt::Write;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Task {
    pub duration: u64,
    pub deps: Vec<usize>,
    pub release: u64,
    pub priority: i32,
    pub needs: Vec<u32>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Run {
    pub start: u64,
    pub finish: u64,
    pub worker: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Schedule {
    pub runs: Vec<Run>,
    pub makespan: u64,
    pub trace: String,
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
    ResourceShape {
        task: usize,
        expected: usize,
        actual: usize,
    },
    ResourceExceeded {
        task: usize,
        resource: usize,
        need: u32,
        capacity: u32,
    },
}

pub fn schedule(
    tasks: &[Task],
    cooldowns: &[u64],
    capacity: &[u32],
) -> Result<Schedule, Error> {
    let n = tasks.len();

    // Validate all dependency entries before any other checks.
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

    let worker_count = cooldowns.len();
    if !(1..=64).contains(&worker_count) {
        return Err(Error::Workers {
            count: worker_count,
        });
    }

    for (task_id, task) in tasks.iter().enumerate() {
        if task.needs.len() != capacity.len() {
            return Err(Error::ResourceShape {
                task: task_id,
                expected: capacity.len(),
                actual: task.needs.len(),
            });
        }
        for (resource, (&need, &cap)) in task.needs.iter().zip(capacity).enumerate() {
            if need > cap {
                return Err(Error::ResourceExceeded {
                    task: task_id,
                    resource,
                    need,
                    capacity: cap,
                });
            }
        }
    }

    // Build predecessor counts and successor lists.
    let mut indegree = vec![0usize; n];
    let mut children = vec![Vec::new(); n];
    for (task_id, task) in tasks.iter().enumerate() {
        indegree[task_id] = task.deps.len();
        for &pred in &task.deps {
            children[pred].push(task_id);
        }
    }

    // Kahn order, always removing the smallest currently available ID.
    let mut ready = BinaryHeap::new();
    for (task_id, &degree) in indegree.iter().enumerate() {
        if degree == 0 {
            ready.push(Reverse(task_id));
        }
    }

    let mut order = Vec::with_capacity(n);
    let mut remaining_indegree = indegree.clone();
    while let Some(Reverse(task_id)) = ready.pop() {
        order.push(task_id);
        for &child in &children[task_id] {
            remaining_indegree[child] -= 1;
            if remaining_indegree[child] == 0 {
                ready.push(Reverse(child));
            }
        }
    }

    if order.len() != n {
        let mut blocked = Vec::new();
        let mut removed = vec![false; n];
        for &task_id in &order {
            removed[task_id] = true;
        }
        for task_id in 0..n {
            if !removed[task_id] {
                blocked.push(task_id);
            }
        }
        return Err(Error::Cycle { blocked });
    }

    // Compute bottom levels in reverse Kahn order.
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

    let mut trace = String::new();
    let _ = writeln!(
        trace,
        "workers={};resources={}",
        worker_count,
        capacity.len()
    );

    let mut used = vec![0u64; capacity.len()];
    let mut completed = vec![false; n];
    let mut started = vec![false; n];
    let mut runs: Vec<Option<Run>> = vec![None; n];
    let mut worker_available = vec![0u64; worker_count];
    let mut running: Vec<(u64, usize, usize)> = Vec::new(); // (finish, task, worker)
    let mut makespan = 0u64;
    let mut time = 0u64;
    let mut completed_count = 0usize;

    while completed_count < n {
        // Complete every task ending now, in ascending task ID.
        let mut due: Vec<(u64, usize, usize)> = Vec::new();
        let mut still_running = Vec::with_capacity(running.len());
        for entry in running.drain(..) {
            if entry.0 == time {
                due.push(entry);
            } else {
                still_running.push(entry);
            }
        }
        running = still_running;
        due.sort_by_key(|&(_, task_id, _)| task_id);

        for (finish, task_id, worker) in due {
            for (resource, &need) in tasks[task_id].needs.iter().enumerate() {
                used[resource] -= u64::from(need);
            }
            completed[task_id] = true;
            completed_count += 1;

            let _ = write!(trace, "{}|end|{}|{}|", finish, task_id, worker);
            append_usage(&mut trace, &used);
            trace.push('\n');
        }

        loop {
            let mut candidates: Vec<usize> = (0..n)
                .filter(|&task_id| {
                    if started[task_id] || tasks[task_id].release > time {
                        return false;
                    }
                    if !tasks[task_id]
                        .deps
                        .iter()
                        .all(|&pred| completed[pred])
                    {
                        return false;
                    }
                    tasks[task_id]
                        .needs
                        .iter()
                        .enumerate()
                        .all(|(resource, &need)| {
                            u64::from(need)
                                <= u64::from(capacity[resource]) - used[resource]
                        })
                })
                .collect();

            candidates.sort_by(|&a, &b| {
                tails[b]
                    .cmp(&tails[a])
                    .then_with(|| tasks[b].priority.cmp(&tasks[a].priority))
                    .then_with(|| a.cmp(&b))
            });

            let Some(task_id) = candidates.first().copied() else {
                break;
            };

            let worker = (0..worker_count)
                .map(|offset| (task_id % worker_count + offset) % worker_count)
                .find(|&worker| worker_available[worker] <= time);

            let Some(worker) = worker else {
                break;
            };

            let finish = time
                .checked_add(tasks[task_id].duration)
                .ok_or(Error::Overflow {
                    task: task_id,
                    stage: Stage::Finish,
                })?;
            let available_at = finish
                .checked_add(cooldowns[worker])
                .ok_or(Error::Overflow {
                    task: task_id,
                    stage: Stage::Turnaround,
                })?;

            for (resource, &need) in tasks[task_id].needs.iter().enumerate() {
                used[resource] += u64::from(need);
            }
            started[task_id] = true;
            worker_available[worker] = available_at;
            runs[task_id] = Some(Run {
                start: time,
                finish,
                worker,
            });
            makespan = makespan.max(finish);

            let _ = write!(trace, "{}|start|{}|{}|", time, task_id, worker);
            append_usage(&mut trace, &used);
            trace.push('\n');

            if finish == time {
                // Zero-duration completions are immediate, before another admission.
                for (resource, &need) in tasks[task_id].needs.iter().enumerate() {
                    used[resource] -= u64::from(need);
                }
                completed[task_id] = true;
                completed_count += 1;

                let _ = write!(trace, "{}|end|{}|{}|", time, task_id, worker);
                append_usage(&mut trace, &used);
                trace.push('\n');
            } else {
                running.push((finish, task_id, worker));
            }
        }

        if completed_count == n {
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
        for task_id in 0..n {
            if !started[task_id] && tasks[task_id].release > time {
                consider(tasks[task_id].release);
            }
        }
        for &available_at in &worker_available {
            if available_at > time {
                consider(available_at);
            }
        }

        // A zero-duration task may have a release at u64::MAX. It is handled by
        // the normal event selection above; no event exists beyond u64::MAX.
        time = next_time.expect("unfinished tasks always have a future event");
    }

    let runs = runs.into_iter().map(Option::unwrap).collect();
    let _ = writeln!(trace, "makespan={}", makespan);

    Ok(Schedule {
        runs,
        makespan,
        trace,
    })
}

fn append_usage(trace: &mut String, used: &[u64]) {
    if used.is_empty() {
        trace.push('-');
        return;
    }
    for (index, value) in used.iter().enumerate() {
        if index > 0 {
            trace.push(',');
        }
        let _ = write!(trace, "{}", value);
    }
}

fn main() {}