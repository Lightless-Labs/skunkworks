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

    // Validate every dependency entry before any other validation.
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
        for (resource, (&need, &available)) in
            task.needs.iter().zip(capacity.iter()).enumerate()
        {
            if need > available {
                return Err(Error::ResourceExceeded {
                    task: task_id,
                    resource,
                    need,
                    capacity: available,
                });
            }
        }
    }

    // Build predecessor counts and successor lists.
    let mut indegree = vec![0usize; n];
    let mut children = vec![Vec::<usize>::new(); n];
    for (task_id, task) in tasks.iter().enumerate() {
        indegree[task_id] = task.deps.len();
        for &pred in &task.deps {
            children[pred].push(task_id);
        }
    }

    // Deterministic Kahn traversal, always removing the smallest available ID.
    let mut remaining_indegree = indegree.clone();
    let mut available = Vec::<usize>::new();
    for id in 0..n {
        if remaining_indegree[id] == 0 {
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
            remaining_indegree[child] -= 1;
            if remaining_indegree[child] == 0 {
                available.push(child);
            }
        }
    }

    if order.len() != n {
        let removed = {
            let mut flags = vec![false; n];
            for &id in &order {
                flags[id] = true;
            }
            flags
        };
        let blocked = (0..n).filter(|&id| !removed[id]).collect();
        return Err(Error::Cycle { blocked });
    }

    // Bottom levels, computed in reverse Kahn order.
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

    let worker_count = cooldowns.len();
    let mut runs: Vec<Option<Run>> = vec![None; n];
    let mut started = vec![false; n];
    let mut completed = vec![false; n];
    let mut used = vec![0u64; capacity.len()];
    let mut worker_available = vec![0u64; worker_count];
    let mut running = Vec::<(u64, usize, usize)>::new(); // (finish, task, worker)

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
        // Complete all tasks finishing now, in ascending task ID.
        let mut finished_now: Vec<(u64, usize, usize)> = Vec::new();
        let mut still_running = Vec::new();
        for entry in running.drain(..) {
            if entry.0 == now {
                finished_now.push(entry);
            } else {
                still_running.push(entry);
            }
        }
        running = still_running;
        finished_now.sort_by_key(|entry| entry.1);

        for &(finish, task_id, worker) in &finished_now {
            for r in 0..capacity.len() {
                used[r] -= tasks[task_id].needs[r] as u64;
            }
            completed[task_id] = true;
            completed_count += 1;
            makespan = makespan.max(finish);

            trace.push_str(&finish.to_string());
            trace.push_str("|end|");
            trace.push_str(&task_id.to_string());
            trace.push('|');
            trace.push_str(&worker.to_string());
            trace.push('|');
            append_used(&mut trace, &used);
            trace.push('\n');
        }

        // Start tasks one at a time, reevaluating after each admission.
        loop {
            let mut eligible: Vec<usize> = (0..n)
                .filter(|&id| {
                    !started[id]
                        && tasks[id].release <= now
                        && tasks[id].deps.iter().all(|&pred| completed[pred])
                        && (0..capacity.len()).all(|r| {
                            (tasks[id].needs[r] as u64)
                                <= (capacity[r] as u64).checked_sub(used[r]).unwrap_or(0)
                        })
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
            for r in 0..capacity.len() {
                used[r] += tasks[task_id].needs[r] as u64;
            }
            runs[task_id] = Some(Run {
                start: now,
                finish,
                worker,
            });

            trace.push_str(&now.to_string());
            trace.push_str("|start|");
            trace.push_str(&task_id.to_string());
            trace.push('|');
            trace.push_str(&worker.to_string());
            trace.push('|');
            append_used(&mut trace, &used);
            trace.push('\n');

            if tasks[task_id].duration == 0 {
                // Immediate completion follows this start before any next start.
                for r in 0..capacity.len() {
                    used[r] -= tasks[task_id].needs[r] as u64;
                }
                completed[task_id] = true;
                completed_count += 1;
                makespan = makespan.max(now);

                trace.push_str(&now.to_string());
                trace.push_str("|end|");
                trace.push_str(&task_id.to_string());
                trace.push('|');
                trace.push_str(&worker.to_string());
                trace.push('|');
                append_used(&mut trace, &used);
                trace.push('\n');
            } else {
                running.push((finish, task_id, worker));
            }
        }

        if completed_count == n {
            break;
        }

        // Advance to the earliest future event that can change eligibility.
        let mut next_time: Option<u64> = None;
        let mut consider = |candidate: u64| {
            if candidate > now && next_time.map_or(true, |current| candidate < current) {
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

        now = next_time.expect("unfinished valid schedule must have a future event");
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

fn append_used(trace: &mut String, used: &[u64]) {
    if used.is_empty() {
        trace.push('-');
    } else {
        for (index, value) in used.iter().enumerate() {
            if index != 0 {
                trace.push(',');
            }
            trace.push_str(&value.to_string());
        }
    }
}

fn main() {}