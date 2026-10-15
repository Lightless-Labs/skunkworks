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
    use std::collections::BTreeSet;

    let n = tasks.len();

    // Validate every dependency before any other check.
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

    if cooldowns.is_empty() || cooldowns.len() > 64 {
        return Err(Error::Workers {
            count: cooldowns.len(),
        });
    }

    // Validate resource shapes and demands in task/input order.
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
    let mut children = vec![Vec::<usize>::new(); n];
    for (task_id, task) in tasks.iter().enumerate() {
        indegree[task_id] = task.deps.len();
        for &dep in &task.deps {
            children[dep].push(task_id);
        }
    }

    // Kahn traversal, always taking the smallest currently available ID.
    let mut available = BTreeSet::new();
    for id in 0..n {
        if indegree[id] == 0 {
            available.insert(id);
        }
    }
    let mut order = Vec::with_capacity(n);
    while let Some(id) = available.pop_first() {
        order.push(id);
        for &child in &children[id] {
            indegree[child] -= 1;
            if indegree[child] == 0 {
                available.insert(child);
            }
        }
    }

    if order.len() != n {
        let removed: Vec<bool> = {
            let mut flags = vec![false; n];
            for &id in &order {
                flags[id] = true;
            }
            flags
        };
        return Err(Error::Cycle {
            blocked: (0..n).filter(|&id| !removed[id]).collect(),
        });
    }

    // Bottom levels, in reverse Kahn order.
    let mut tail = vec![0u64; n];
    for &id in order.iter().rev() {
        let max_child = children[id]
            .iter()
            .map(|&child| tail[child])
            .max()
            .unwrap_or(0);
        tail[id] = tasks[id]
            .duration
            .checked_add(max_child)
            .ok_or(Error::Overflow {
                task: id,
                stage: Stage::Tail,
            })?;
    }

    #[derive(Clone, Copy)]
    struct Running {
        task: usize,
        finish: u64,
        worker: usize,
    }

    let worker_count = cooldowns.len();
    let mut worker_available = vec![0u64; worker_count];
    let mut running = Vec::<Running>::new();
    let mut completed = vec![false; n];
    let mut started = vec![false; n];
    let mut used = vec![0u64; capacity.len()];
    let mut runs: Vec<Option<Run>> = vec![None; n];
    let mut trace = format!("workers={};resources={}\n", worker_count, capacity.len());
    let mut now = 0u64;
    let mut completed_count = 0usize;
    let mut makespan = 0u64;

    fn append_usage(out: &mut String, used: &[u64]) {
        if used.is_empty() {
            out.push('-');
        } else {
            for (i, value) in used.iter().enumerate() {
                if i != 0 {
                    out.push(',');
                }
                out.push_str(&value.to_string());
            }
        }
    }

    while completed_count < n {
        // Complete all tasks finishing now, in ascending task ID order.
        let mut ending: Vec<Running> = running
            .iter()
            .copied()
            .filter(|item| item.finish == now)
            .collect();
        ending.sort_by_key(|item| item.task);

        for item in ending {
            running.retain(|other| other.task != item.task);
            completed[item.task] = true;
            completed_count += 1;
            makespan = makespan.max(item.finish);
            for (r, &need) in tasks[item.task].needs.iter().enumerate() {
                used[r] -= u64::from(need);
            }
            trace.push_str(&format!(
                "{}|end|{}|{}|",
                now, item.task, item.worker
            ));
            append_usage(&mut trace, &used);
            trace.push('\n');
        }

        // Repeatedly start one currently eligible, fitting task.
        let mut started_any = false;
        loop {
            let mut candidates: Vec<usize> = (0..n)
                .filter(|&id| {
                    !started[id]
                        && tasks[id].release <= now
                        && tasks[id].deps.iter().all(|&dep| completed[dep])
                        && tasks[id].needs.iter().enumerate().all(|(r, &need)| {
                            u64::from(need) <= u64::from(capacity[r]) - used[r]
                        })
                })
                .collect();

            candidates.sort_by(|&a, &b| {
                tail[b]
                    .cmp(&tail[a])
                    .then_with(|| tasks[b].priority.cmp(&tasks[a].priority))
                    .then_with(|| a.cmp(&b))
            });

            let Some(task_id) = candidates.first().copied() else {
                break;
            };

            // Choose the first available worker in the task's cyclic preference.
            let mut chosen_worker = None;
            for offset in 0..worker_count {
                let worker = (task_id % worker_count + offset) % worker_count;
                if worker_available[worker] <= now {
                    chosen_worker = Some(worker);
                    break;
                }
            }
            let Some(worker) = chosen_worker else {
                break;
            };

            let finish = now
                .checked_add(tasks[task_id].duration)
                .ok_or(Error::Overflow {
                    task: task_id,
                    stage: Stage::Finish,
                })?;
            let worker_ready = finish
                .checked_add(cooldowns[worker])
                .ok_or(Error::Overflow {
                    task: task_id,
                    stage: Stage::Turnaround,
                })?;

            started[task_id] = true;
            worker_available[worker] = worker_ready;
            for (r, &need) in tasks[task_id].needs.iter().enumerate() {
                used[r] += u64::from(need);
            }
            runs[task_id] = Some(Run {
                start: now,
                finish,
                worker,
            });
            trace.push_str(&format!("{}|start|{}|{}|", now, task_id, worker));
            append_usage(&mut trace, &used);
            trace.push('\n');
            started_any = true;

            if finish == now {
                completed[task_id] = true;
                completed_count += 1;
                makespan = makespan.max(finish);
                for (r, &need) in tasks[task_id].needs.iter().enumerate() {
                    used[r] -= u64::from(need);
                }
                trace.push_str(&format!("{}|end|{}|{}|", now, task_id, worker));
                append_usage(&mut trace, &used);
                trace.push('\n');
            } else {
                running.push(Running {
                    task: task_id,
                    finish,
                    worker,
                });
            }
        }

        if completed_count == n {
            break;
        }

        if started_any {
            // All possible starts at this instant have been reconsidered.
        }

        let mut next_time: Option<u64> = None;
        let mut consider = |candidate: u64| {
            if candidate > now {
                next_time = Some(next_time.map_or(candidate, |current| current.min(candidate)));
            }
        };

        for item in &running {
            consider(item.finish);
        }
        for id in 0..n {
            if !started[id] {
                consider(tasks[id].release);
            }
        }
        for worker in 0..worker_count {
            if worker_available[worker] > now {
                consider(worker_available[worker]);
            }
        }

        if let Some(next) = next_time {
            now = next;
        } else {
            // The graph is acyclic, so only an unavailable worker or a future
            // release can prevent progress. This fallback is unreachable for
            // valid states but avoids panicking if invariants are violated.
            break;
        }
    }

    let runs = runs
        .into_iter()
        .map(|run| run.expect("all tasks were started"))
        .collect();

    trace.push_str(&format!("makespan={}\n", makespan));
    Ok(Schedule {
        runs,
        makespan,
        trace,
    })
}

fn main() {}