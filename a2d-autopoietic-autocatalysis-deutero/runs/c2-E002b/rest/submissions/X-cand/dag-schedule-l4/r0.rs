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

    // Validate dependencies in input order before other validation.
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

    if cooldowns.is_empty() || cooldowns.len() > 64 {
        return Err(Error::Workers {
            count: cooldowns.len(),
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

    // Build successor lists and indegrees.
    let mut successors = vec![Vec::<usize>::new(); n];
    let mut indegree = vec![0usize; n];
    for (task_id, task) in tasks.iter().enumerate() {
        indegree[task_id] = task.deps.len();
        for &dep in &task.deps {
            successors[dep].push(task_id);
        }
    }

    // Kahn traversal, always removing the smallest currently available ID.
    let mut available = Vec::<usize>::new();
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
        for &child in &successors[id] {
            indegree[child] -= 1;
            if indegree[child] == 0 {
                available.push(child);
            }
        }
    }
    if order.len() != n {
        let removed = {
            let mut removed = vec![false; n];
            for &id in &order {
                removed[id] = true;
            }
            removed
        };
        return Err(Error::Cycle {
            blocked: (0..n).filter(|&id| !removed[id]).collect(),
        });
    }

    // Compute bottom levels in reverse Kahn order.
    let mut tails = vec![0u64; n];
    for &id in order.iter().rev() {
        let max_child = successors[id]
            .iter()
            .map(|&child| tails[child])
            .max()
            .unwrap_or(0);
        tails[id] = tasks[id]
            .duration
            .checked_add(max_child)
            .ok_or(Error::Overflow {
                task: id,
                stage: Stage::Tail,
            })?;
    }

    let worker_count = cooldowns.len();
    let resource_count = capacity.len();
    let mut used = vec![0u64; resource_count];
    let mut worker_available = vec![0u64; worker_count];
    let mut started = vec![false; n];
    let mut completed = vec![false; n];
    let mut remaining_deps: Vec<usize> = tasks.iter().map(|t| t.deps.len()).collect();
    let mut runs: Vec<Option<Run>> = vec![None; n];
    let mut running = Vec::<(u64, usize, usize)>::new(); // (finish, task, worker)
    let mut trace = String::new();
    trace.push_str(&format!(
        "workers={};resources={}\n",
        worker_count, resource_count
    ));

    let mut now = 0u64;
    let mut makespan = 0u64;
    let mut completed_count = 0usize;

    while completed_count < n {
        // Complete all tasks due now, in task ID order.
        let mut due: Vec<(u64, usize, usize)> = running
            .iter()
            .copied()
            .filter(|(finish, _, _)| *finish == now)
            .collect();
        due.sort_by_key(|(_, task, _)| *task);

        for (finish, task_id, worker) in due {
            if let Some(pos) = running
                .iter()
                .position(|entry| *entry == (finish, task_id, worker))
            {
                running.swap_remove(pos);
            }
            completed[task_id] = true;
            completed_count += 1;
            makespan = makespan.max(finish);

            for r in 0..resource_count {
                used[r] -= tasks[task_id].needs[r] as u64;
            }
            append_event(&mut trace, now, "end", task_id, worker, &used);

            for &child in &successors[task_id] {
                remaining_deps[child] -= 1;
            }
        }

        // Admit one task at a time, reevaluating after every start/completion.
        loop {
            let mut candidates: Vec<usize> = (0..n)
                .filter(|&id| {
                    !started[id]
                        && tasks[id].release <= now
                        && remaining_deps[id] == 0
                        && fits(&tasks[id].needs, &used, capacity)
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
            for r in 0..resource_count {
                used[r] += tasks[task_id].needs[r] as u64;
            }
            runs[task_id] = Some(Run {
                start: now,
                finish,
                worker,
            });
            append_event(&mut trace, now, "start", task_id, worker, &used);

            if finish == now {
                completed[task_id] = true;
                completed_count += 1;
                makespan = makespan.max(finish);
                for r in 0..resource_count {
                    used[r] -= tasks[task_id].needs[r] as u64;
                }
                append_event(&mut trace, now, "end", task_id, worker, &used);
                for &child in &successors[task_id] {
                    remaining_deps[child] -= 1;
                }
            } else {
                running.push((finish, task_id, worker));
            }
        }

        if completed_count == n {
            break;
        }

        // Advance to the next finish, release, or worker availability event.
        let mut next = None::<u64>;
        for &(finish, _, _) in &running {
            if finish > now {
                next = Some(next.map_or(finish, |v| v.min(finish)));
            }
        }
        for id in 0..n {
            if !started[id] && tasks[id].release > now {
                next = Some(next.map_or(tasks[id].release, |v| v.min(tasks[id].release)));
            }
        }

        // A worker expiry matters only if some task could otherwise be admitted
        // once its release and dependencies allow it.
        for &expiry in &worker_available {
            if expiry > now {
                let has_ready_task = (0..n).any(|id| {
                    !started[id]
                        && tasks[id].release <= expiry
                        && remaining_deps[id] == 0
                        && fits(&tasks[id].needs, &used, capacity)
                });
                if has_ready_task {
                    next = Some(next.map_or(expiry, |v| v.min(expiry)));
                }
            }
        }

        if let Some(next_time) = next {
            now = next_time;
        } else {
            // With an acyclic graph, this can only occur if a task is waiting
            // on an event at u64::MAX. Move there if it is a valid future event.
            let future = (0..n)
                .filter(|&id| !started[id] && tasks[id].release > now)
                .map(|id| tasks[id].release)
                .chain(running.iter().map(|(finish, _, _)| *finish))
                .chain(worker_available.iter().copied().filter(|&t| t > now))
                .min();
            if let Some(t) = future {
                now = t;
            } else {
                break;
            }
        }
    }

    for (id, slot) in runs.iter().enumerate() {
        if slot.is_none() {
            // This is unreachable for a valid acyclic graph under the event rules.
            let _ = id;
        }
    }

    trace.push_str(&format!("makespan={}\n", makespan));
    Ok(Schedule {
        runs: runs.into_iter().map(Option::unwrap).collect(),
        makespan,
        trace,
    })
}

fn fits(needs: &[u32], used: &[u64], capacity: &[u32]) -> bool {
    needs
        .iter()
        .zip(used.iter().zip(capacity))
        .all(|(&need, (&in_use, &cap))| (need as u64) <= (cap as u64) - in_use)
}

fn append_event(
    trace: &mut String,
    time: u64,
    kind: &str,
    task: usize,
    worker: usize,
    used: &[u64],
) {
    use std::fmt::Write;

    let _ = write!(trace, "{}|{}|{}|{}|", time, kind, task, worker);
    if used.is_empty() {
        trace.push('-');
    } else {
        for (index, amount) in used.iter().enumerate() {
            if index != 0 {
                trace.push(',');
            }
            let _ = write!(trace, "{}", amount);
        }
    }
    trace.push('\n');
}