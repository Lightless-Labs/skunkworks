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

    if cooldowns.is_empty() || cooldowns.len() > 64 {
        return Err(Error::Workers {
            count: cooldowns.len(),
        });
    }

    // Validate resource shapes and demands in task order.
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
    let mut children = vec![Vec::<usize>::new(); n];
    let mut indegree = vec![0usize; n];
    for (task_id, task) in tasks.iter().enumerate() {
        indegree[task_id] = task.deps.len();
        for &pred in &task.deps {
            children[pred].push(task_id);
        }
    }

    // Kahn order, always removing the smallest currently available task ID.
    let mut available = vec![false; n];
    for id in 0..n {
        if indegree[id] == 0 {
            available[id] = true;
        }
    }
    let mut removed = vec![false; n];
    let mut order = Vec::with_capacity(n);
    while let Some(id) = (0..n).find(|&id| available[id]) {
        available[id] = false;
        removed[id] = true;
        order.push(id);
        for &child in &children[id] {
            indegree[child] -= 1;
            if indegree[child] == 0 {
                available[child] = true;
            }
        }
    }

    if order.len() != n {
        let blocked = (0..n).filter(|&id| !removed[id]).collect();
        return Err(Error::Cycle { blocked });
    }

    // Compute bottom levels in reverse Kahn order.
    let mut tails = vec![0u64; n];
    for &id in order.iter().rev() {
        let max_child = children[id]
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
    let mut runs: Vec<Option<Run>> = vec![None; n];
    let mut started = vec![false; n];
    let mut completed = vec![false; n];
    let mut remaining_preds: Vec<usize> = tasks.iter().map(|t| t.deps.len()).collect();
    let mut used = vec![0u64; resource_count];
    let mut worker_available = vec![0u64; worker_count];

    // (finish time, task ID, worker ID)
    let mut running: Vec<(u64, usize, usize)> = Vec::new();
    let mut trace = format!("workers={worker_count};resources={resource_count}\n");
    let mut now = 0u64;
    let mut completed_count = 0usize;
    let mut makespan = 0u64;

    while completed_count < n {
        // Complete all tasks finishing now, in ascending task ID.
        let mut finishing: Vec<(u64, usize, usize)> = running
            .iter()
            .copied()
            .filter(|&(finish, _, _)| finish == now)
            .collect();
        finishing.sort_by_key(|&(_, task_id, _)| task_id);

        if !finishing.is_empty() {
            running.retain(|&(finish, _, _)| finish != now);
            for &(_, task_id, worker) in &finishing {
                completed[task_id] = true;
                completed_count += 1;
                makespan = makespan.max(now);
                for r in 0..resource_count {
                    used[r] -= u64::from(tasks[task_id].needs[r]);
                }
                for &child in &children[task_id] {
                    remaining_preds[child] -= 1;
                }
                append_event(&mut trace, now, "end", task_id, worker, &used);
            }
        }

        // Start one task at a time, re-evaluating after each start or
        // immediate completion of a zero-duration task.
        let mut did_start = false;
        loop {
            let mut candidates: Vec<usize> = (0..n)
                .filter(|&id| {
                    if started[id] || tasks[id].release > now || remaining_preds[id] != 0 {
                        return false;
                    }
                    (0..resource_count).all(|r| {
                        u64::from(tasks[id].needs[r]) <= u64::from(capacity[r]) - used[r]
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

            for r in 0..resource_count {
                used[r] += u64::from(tasks[task_id].needs[r]);
            }
            started[task_id] = true;
            worker_available[worker] = available_at;
            runs[task_id] = Some(Run {
                start: now,
                finish,
                worker,
            });
            append_event(&mut trace, now, "start", task_id, worker, &used);
            did_start = true;

            if tasks[task_id].duration == 0 {
                completed[task_id] = true;
                completed_count += 1;
                makespan = makespan.max(now);
                for r in 0..resource_count {
                    used[r] -= u64::from(tasks[task_id].needs[r]);
                }
                for &child in &children[task_id] {
                    remaining_preds[child] -= 1;
                }
                append_event(&mut trace, now, "end", task_id, worker, &used);
            } else {
                running.push((finish, task_id, worker));
            }
        }

        if completed_count == n {
            break;
        }

        // If a task started at this time, the loop above already exhausted
        // all currently startable tasks. Find the next strictly later event.
        let mut next_time: Option<u64> = None;
        let mut consider = |time: u64| {
            if time > now {
                next_time = Some(next_time.map_or(time, |current| current.min(time)));
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

        if let Some(time) = next_time {
            now = time;
        } else if !did_start {
            // This should be unreachable for an acyclic graph with valid
            // arithmetic, but prevents a nonterminating loop if invariants
            // are ever changed.
            break;
        }
    }

    trace.push_str(&format!("makespan={makespan}\n"));
    let runs = runs
        .into_iter()
        .map(|run| run.expect("acyclic task must start and complete"))
        .collect();

    Ok(Schedule {
        runs,
        makespan,
        trace,
    })
}

fn append_event(
    trace: &mut String,
    time: u64,
    event: &str,
    task_id: usize,
    worker: usize,
    used: &[u64],
) {
    trace.push_str(&format!("{time}|{event}|{task_id}|{worker}|"));
    if used.is_empty() {
        trace.push_str("-\n");
    } else {
        for (index, value) in used.iter().enumerate() {
            if index != 0 {
                trace.push(',');
            }
            trace.push_str(&value.to_string());
        }
        trace.push('\n');
    }
}

fn main() {}