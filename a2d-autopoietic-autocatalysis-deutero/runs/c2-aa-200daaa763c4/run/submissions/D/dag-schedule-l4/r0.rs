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

fn append_resource_snapshot(trace: &mut String, used: &[u128]) {
    if used.is_empty() {
        trace.push('-');
    } else {
        for (i, value) in used.iter().enumerate() {
            if i > 0 {
                trace.push(',');
            }
            trace.push_str(&value.to_string());
        }
    }
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

    let worker_count = cooldowns.len();
    if !(1..=64).contains(&worker_count) {
        return Err(Error::Workers {
            count: worker_count,
        });
    }

    // Validate resource shapes and demands task by task.
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
    let mut indegree: Vec<usize> = tasks.iter().map(|task| task.deps.len()).collect();
    let mut children = vec![Vec::new(); n];
    for (task_id, task) in tasks.iter().enumerate() {
        for &pred in &task.deps {
            children[pred].push(task_id);
        }
    }

    // Kahn traversal, always removing the smallest currently available ID.
    let mut available = Vec::new();
    for (task_id, &degree) in indegree.iter().enumerate() {
        if degree == 0 {
            available.push(task_id);
        }
    }

    let mut topo = Vec::with_capacity(n);
    while !available.is_empty() {
        let smallest_pos = available
            .iter()
            .enumerate()
            .min_by_key(|(_, id)| **id)
            .map(|(pos, _)| pos)
            .expect("available is nonempty");
        let id = available.swap_remove(smallest_pos);
        topo.push(id);

        for &child in &children[id] {
            indegree[child] -= 1;
            if indegree[child] == 0 {
                available.push(child);
            }
        }
    }

    if topo.len() != n {
        let removed: Vec<bool> = {
            let mut v = vec![false; n];
            for &id in &topo {
                v[id] = true;
            }
            v
        };
        return Err(Error::Cycle {
            blocked: (0..n).filter(|&id| !removed[id]).collect(),
        });
    }

    // Compute bottom levels in reverse Kahn order.
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

    let mut trace = String::new();
    trace.push_str("workers=");
    trace.push_str(&worker_count.to_string());
    trace.push_str(";resources=");
    trace.push_str(&capacity.len().to_string());
    trace.push('\n');

    let mut used = vec![0u128; capacity.len()];
    let mut started = vec![false; n];
    let mut completed = vec![false; n];
    let mut completed_count = 0usize;
    let mut runs: Vec<Option<Run>> = vec![None; n];

    #[derive(Clone)]
    struct Running {
        task: usize,
        worker: usize,
        finish: u64,
    }

    let mut running: Vec<Running> = Vec::new();
    let mut worker_available = vec![0u64; worker_count];
    let mut now = 0u64;
    let mut makespan = 0u64;

    while completed_count < n {
        // Complete all tasks finishing now, ordered by task ID.
        let mut finished_now: Vec<usize> = running
            .iter()
            .enumerate()
            .filter_map(|(i, r)| (r.finish == now).then_some(i))
            .collect();
        finished_now.sort_by_key(|&i| running[i].task);

        for index in finished_now.into_iter().rev() {
            let item = running.remove(index);
            for (resource, &need) in tasks[item.task].needs.iter().enumerate() {
                used[resource] -= u128::from(need);
            }
            completed[item.task] = true;
            completed_count += 1;
            makespan = makespan.max(item.finish);

            trace.push_str(&now.to_string());
            trace.push_str("|end|");
            trace.push_str(&item.task.to_string());
            trace.push('|');
            trace.push_str(&item.worker.to_string());
            trace.push('|');
            append_resource_snapshot(&mut trace, &used);
            trace.push('\n');
        }

        // Start tasks one at a time, re-ranking after every start and
        // immediate completion.
        loop {
            let mut best: Option<usize> = None;
            for id in 0..n {
                if started[id] || tasks[id].release > now {
                    continue;
                }
                if !tasks[id].deps.iter().all(|&pred| completed[pred]) {
                    continue;
                }
                let fits = tasks[id]
                    .needs
                    .iter()
                    .enumerate()
                    .all(|(r, &need)| u128::from(need) <= u128::from(capacity[r]) - used[r]);
                if !fits {
                    continue;
                }
                let has_worker = worker_available.iter().any(|&t| t <= now);
                if !has_worker {
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

            let Some(id) = best else {
                break;
            };

            let preferred = id % worker_count;
            let worker = (0..worker_count)
                .map(|offset| (preferred + offset) % worker_count)
                .find(|&w| worker_available[w] <= now)
                .expect("a worker was checked as available");

            let finish = now
                .checked_add(tasks[id].duration)
                .ok_or(Error::Overflow {
                    task: id,
                    stage: Stage::Finish,
                })?;
            let available_at = finish
                .checked_add(cooldowns[worker])
                .ok_or(Error::Overflow {
                    task: id,
                    stage: Stage::Turnaround,
                })?;

            started[id] = true;
            worker_available[worker] = available_at;
            for (resource, &need) in tasks[id].needs.iter().enumerate() {
                used[resource] += u128::from(need);
            }
            runs[id] = Some(Run {
                start: now,
                finish,
                worker,
            });

            trace.push_str(&now.to_string());
            trace.push_str("|start|");
            trace.push_str(&id.to_string());
            trace.push('|');
            trace.push_str(&worker.to_string());
            trace.push('|');
            append_resource_snapshot(&mut trace, &used);
            trace.push('\n');

            if tasks[id].duration == 0 {
                for (resource, &need) in tasks[id].needs.iter().enumerate() {
                    used[resource] -= u128::from(need);
                }
                completed[id] = true;
                completed_count += 1;

                trace.push_str(&now.to_string());
                trace.push_str("|end|");
                trace.push_str(&id.to_string());
                trace.push('|');
                trace.push_str(&worker.to_string());
                trace.push('|');
                append_resource_snapshot(&mut trace, &used);
                trace.push('\n');
            } else {
                running.push(Running {
                    task: id,
                    worker,
                    finish,
                });
            }
        }

        if completed_count == n {
            break;
        }

        // No task can start now. Advance to the earliest relevant future event.
        let mut next_time: Option<u64> = None;
        for item in &running {
            if item.finish > now {
                next_time = Some(next_time.map_or(item.finish, |t| t.min(item.finish)));
            }
        }
        for id in 0..n {
            if !started[id] && tasks[id].release > now {
                next_time = Some(next_time.map_or(tasks[id].release, |t| t.min(tasks[id].release)));
            }
        }
        for &time in &worker_available {
            if time > now {
                next_time = Some(next_time.map_or(time, |t| t.min(time)));
            }
        }

        // In an acyclic nonempty graph, some running task, release, or worker
        // availability event must eventually make progress possible.
        now = next_time.expect("unfinished tasks have a future event");
    }

    trace.push_str("makespan=");
    trace.push_str(&makespan.to_string());
    trace.push('\n');

    let runs = runs
        .into_iter()
        .map(|run| run.expect("every task starts exactly once"))
        .collect();

    Ok(Schedule {
        runs,
        makespan,
        trace,
    })
}