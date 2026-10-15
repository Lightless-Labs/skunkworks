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
    BadDependency { task: usize, position: usize, value: usize },
    SelfDependency { task: usize, position: usize },
    DuplicateDependency { task: usize, position: usize, value: usize },
    Cycle { blocked: Vec<usize> },
    Workers { count: usize },
    Overflow { task: usize, stage: Stage },
    ResourceShape { task: usize, expected: usize, actual: usize },
    ResourceExceeded { task: usize, resource: usize, need: u32, capacity: u32 },
}

fn append_resources(trace: &mut String, used: &[u128]) {
    if used.is_empty() {
        trace.push('-');
    } else {
        for (i, amount) in used.iter().enumerate() {
            if i != 0 {
                trace.push(',');
            }
            trace.push_str(&amount.to_string());
        }
    }
}

fn append_event(
    trace: &mut String,
    time: u64,
    kind: &str,
    task: usize,
    worker: usize,
    used: &[u128],
) {
    trace.push_str(&time.to_string());
    trace.push('|');
    trace.push_str(kind);
    trace.push('|');
    trace.push_str(&task.to_string());
    trace.push('|');
    trace.push_str(&worker.to_string());
    trace.push('|');
    append_resources(trace, used);
    trace.push('\n');
}

pub fn schedule(
    tasks: &[Task],
    cooldowns: &[u64],
    capacity: &[u32],
) -> Result<Schedule, Error> {
    let n = tasks.len();

    // Validate every dependency entry before any other validation.
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

    if cooldowns.is_empty() || cooldowns.len() > 64 {
        return Err(Error::Workers {
            count: cooldowns.len(),
        });
    }

    // Resource validation is task-major, then resource-major.
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

    // Kahn traversal with the smallest available task ID first.
    let mut indegree: Vec<usize> = tasks.iter().map(|task| task.deps.len()).collect();
    let mut children = vec![Vec::new(); n];
    for (task_id, task) in tasks.iter().enumerate() {
        for &dep in &task.deps {
            children[dep].push(task_id);
        }
    }

    let mut available = std::collections::BTreeSet::new();
    for (task_id, &degree) in indegree.iter().enumerate() {
        if degree == 0 {
            available.insert(task_id);
        }
    }

    let mut order = Vec::with_capacity(n);
    while let Some(&task_id) = available.iter().next() {
        available.remove(&task_id);
        order.push(task_id);
        for &child in &children[task_id] {
            indegree[child] -= 1;
            if indegree[child] == 0 {
                available.insert(child);
            }
        }
    }

    if order.len() != n {
        let mut removed = vec![false; n];
        for &task_id in &order {
            removed[task_id] = true;
        }
        return Err(Error::Cycle {
            blocked: (0..n).filter(|&id| !removed[id]).collect(),
        });
    }

    // Compute bottom-level tails in reverse Kahn order.
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

    let worker_count = cooldowns.len();
    let mut runs: Vec<Option<Run>> = vec![None; n];
    let mut started = vec![false; n];
    let mut completed = vec![false; n];
    let mut remaining_deps: Vec<usize> = tasks.iter().map(|task| task.deps.len()).collect();
    let mut used = vec![0u128; capacity.len()];
    let mut worker_available = vec![0u64; worker_count];

    #[derive(Clone)]
    struct Running {
        task: usize,
        worker: usize,
        finish: u64,
    }

    let mut running: Vec<Running> = Vec::new();
    let mut trace = String::new();
    trace.push_str("workers=");
    trace.push_str(&worker_count.to_string());
    trace.push_str(";resources=");
    trace.push_str(&capacity.len().to_string());
    trace.push('\n');

    let mut now = 0u64;
    let mut completed_count = 0usize;
    let mut makespan = 0u64;

    while completed_count < n {
        // Complete all tasks ending now, in ascending task ID order.
        let mut ending: Vec<usize> = running
            .iter()
            .enumerate()
            .filter_map(|(i, item)| (item.finish == now).then_some(i))
            .collect();
        ending.sort_by_key(|&i| running[i].task);

        for index in ending.into_iter().rev() {
            let item = running.remove(index);
            let task_id = item.task;
            for (resource, &need) in tasks[task_id].needs.iter().enumerate() {
                used[resource] -= need as u128;
            }
            completed[task_id] = true;
            completed_count += 1;
            makespan = makespan.max(item.finish);
            append_event(&mut trace, now, "end", task_id, item.worker, &used);

            for &child in &children[task_id] {
                remaining_deps[child] -= 1;
            }
        }

        // Start one eligible fitting task at a time, re-ranking after each.
        loop {
            let mut candidate: Option<usize> = None;
            for task_id in 0..n {
                if started[task_id]
                    || tasks[task_id].release > now
                    || remaining_deps[task_id] != 0
                {
                    continue;
                }
                if tasks[task_id]
                    .needs
                    .iter()
                    .enumerate()
                    .any(|(r, &need)| used[r] + need as u128 > capacity[r] as u128)
                {
                    continue;
                }

                let better = match candidate {
                    None => true,
                    Some(current) => {
                        tails[task_id] > tails[current]
                            || (tails[task_id] == tails[current]
                                && (tasks[task_id].priority > tasks[current].priority
                                    || (tasks[task_id].priority == tasks[current].priority
                                        && task_id < current)))
                    }
                };
                if better {
                    candidate = Some(task_id);
                }
            }

            let Some(task_id) = candidate else {
                break;
            };

            let preferred = task_id % worker_count;
            let worker = (0..worker_count)
                .map(|offset| (preferred + offset) % worker_count)
                .find(|&w| worker_available[w] <= now);

            let Some(worker) = worker else {
                break;
            };

            let finish = now
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

            started[task_id] = true;
            worker_available[worker] = available_at;
            for (resource, &need) in tasks[task_id].needs.iter().enumerate() {
                used[resource] += need as u128;
            }
            runs[task_id] = Some(Run {
                start: now,
                finish,
                worker,
            });
            append_event(&mut trace, now, "start", task_id, worker, &used);

            if finish == now {
                // Immediate completion of zero-duration work.
                for (resource, &need) in tasks[task_id].needs.iter().enumerate() {
                    used[resource] -= need as u128;
                }
                completed[task_id] = true;
                completed_count += 1;
                makespan = makespan.max(finish);
                append_event(&mut trace, now, "end", task_id, worker, &used);
                for &child in &children[task_id] {
                    remaining_deps[child] -= 1;
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

        // Advance to the earliest strictly later event.
        let mut next: Option<u64> = None;
        let mut consider = |time: u64| {
            if time > now {
                next = Some(next.map_or(time, |current| current.min(time)));
            }
        };

        for item in &running {
            consider(item.finish);
        }
        for task_id in 0..n {
            if !started[task_id] && tasks[task_id].release > now {
                consider(tasks[task_id].release);
            }
        }
        for &available_at in &worker_available {
            consider(available_at);
        }

        // A non-cyclic graph always has a runnable future event unless all tasks
        // have completed. This fallback avoids panicking if invariants are broken.
        now = next.unwrap_or(now);
    }

    trace.push_str("makespan=");
    trace.push_str(&makespan.to_string());
    trace.push('\n');

    Ok(Schedule {
        runs: runs.into_iter().map(Option::unwrap).collect(),
        makespan,
        trace,
    })
}

fn main() {}