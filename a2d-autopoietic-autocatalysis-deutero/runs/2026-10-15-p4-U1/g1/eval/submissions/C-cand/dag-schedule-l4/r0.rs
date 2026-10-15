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

    // Validate every dependency in the required order before any other checks.
    for (task_id, task) in tasks.iter().enumerate() {
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
            if task.deps[..position].contains(&value) {
                return Err(Error::DuplicateDependency {
                    task: task_id,
                    position,
                    value,
                });
            }
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

    // Build successor lists and indegrees. Duplicate dependencies have already
    // been rejected, so each edge appears exactly once.
    let mut children = vec![Vec::<usize>::new(); n];
    let mut indegree = vec![0usize; n];
    for (task_id, task) in tasks.iter().enumerate() {
        indegree[task_id] = task.deps.len();
        for &pred in &task.deps {
            children[pred].push(task_id);
        }
    }

    // Kahn traversal with the smallest available task ID chosen each time.
    let mut available = Vec::<usize>::new();
    for id in 0..n {
        if indegree[id] == 0 {
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

        for &child in &children[id] {
            indegree[child] -= 1;
            if indegree[child] == 0 {
                available.push(child);
            }
        }
    }

    if topo.len() != n {
        let mut blocked = Vec::new();
        let mut removed = vec![false; n];
        for &id in &topo {
            removed[id] = true;
        }
        for id in 0..n {
            if !removed[id] {
                blocked.push(id);
            }
        }
        return Err(Error::Cycle { blocked });
    }

    // Compute bottom levels in reverse Kahn order.
    let mut tails = vec![0u64; n];
    for &id in topo.iter().rev() {
        let mut max_child_tail = 0u64;
        for &child in &children[id] {
            max_child_tail = max_child_tail.max(tails[child]);
        }
        tails[id] = tasks[id]
            .duration
            .checked_add(max_child_tail)
            .ok_or(Error::Overflow {
                task: id,
                stage: Stage::Tail,
            })?;
    }

    let resource_count = capacity.len();
    let mut used = vec![0u64; resource_count];
    let mut completed = vec![false; n];
    let mut started = vec![false; n];
    let mut runs: Vec<Option<Run>> = vec![None; n];

    #[derive(Clone)]
    struct Running {
        task: usize,
        worker: usize,
        finish: u64,
        available_at: u64,
    }

    let mut running = Vec::<Running>::new();
    let mut worker_available = vec![0u64; worker_count];
    let mut trace = String::new();
    trace.push_str(&format!(
        "workers={};resources={}\n",
        worker_count, resource_count
    ));

    let mut now = 0u64;
    let mut completed_count = 0usize;
    let mut makespan = 0u64;

    while completed_count < n {
        // Complete all tasks finishing now, in ascending task ID order.
        let mut ending: Vec<usize> = running
            .iter()
            .enumerate()
            .filter_map(|(idx, entry)| (entry.finish == now).then_some(idx))
            .collect();
        ending.sort_by_key(|&idx| running[idx].task);

        for idx in ending.into_iter().rev() {
            let entry = running.remove(idx);
            completed[entry.task] = true;
            completed_count += 1;
            makespan = makespan.max(entry.finish);
            for r in 0..resource_count {
                used[r] -= u64::from(tasks[entry.task].needs[r]);
            }
            trace.push_str(&format!("{}|end|{}|{}|", now, entry.task, entry.worker));
            append_resources(&mut trace, &used);
            trace.push('\n');
        }

        loop {
            let mut best: Option<usize> = None;
            for id in 0..n {
                if started[id] || tasks[id].release > now {
                    continue;
                }
                if !tasks[id].deps.iter().all(|&pred| completed[pred]) {
                    continue;
                }
                let fits = (0..resource_count).all(|r| {
                    u64::from(tasks[id].needs[r]) <= u64::from(capacity[r]) - used[r]
                });
                if !fits {
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

            let worker_start = task_id % worker_count;
            let worker = (0..worker_count)
                .map(|offset| (worker_start + offset) % worker_count)
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
                used[r] += u64::from(tasks[task_id].needs[r]);
            }
            runs[task_id] = Some(Run {
                start: now,
                finish,
                worker,
            });

            trace.push_str(&format!("{}|start|{}|{}|", now, task_id, worker));
            append_resources(&mut trace, &used);
            trace.push('\n');

            running.push(Running {
                task: task_id,
                worker,
                finish,
                available_at,
            });

            // A zero-duration task completes immediately before another start.
            if finish == now {
                let idx = running.len() - 1;
                let entry = running.remove(idx);
                completed[entry.task] = true;
                completed_count += 1;
                makespan = makespan.max(entry.finish);
                for r in 0..resource_count {
                    used[r] -= u64::from(tasks[entry.task].needs[r]);
                }
                trace.push_str(&format!("{}|end|{}|{}|", now, entry.task, entry.worker));
                append_resources(&mut trace, &used);
                trace.push('\n');
            }
        }

        if completed_count == n {
            break;
        }

        let mut next_time: Option<u64> = None;
        for entry in &running {
            if entry.finish > now {
                next_time = Some(next_time.map_or(entry.finish, |t| t.min(entry.finish)));
            }
        }
        for id in 0..n {
            if !started[id] && tasks[id].release > now {
                next_time = Some(next_time.map_or(tasks[id].release, |t| t.min(tasks[id].release)));
            }
        }
        for w in 0..worker_count {
            if worker_available[w] > now {
                next_time = Some(next_time.map_or(worker_available[w], |t| t.min(worker_available[w])));
            }
        }

        // If all work has started but only zero-duration completions were
        // processed, the loop is complete; otherwise some future event exists.
        if let Some(t) = next_time {
            now = t;
        } else {
            break;
        }
    }

    let runs = runs
        .into_iter()
        .map(|run| run.expect("every task is started exactly once"))
        .collect();

    trace.push_str(&format!("makespan={}\n", makespan));

    Ok(Schedule {
        runs,
        makespan,
        trace,
    })
}

fn append_resources(trace: &mut String, used: &[u64]) {
    if used.is_empty() {
        trace.push('-');
    } else {
        for (index, amount) in used.iter().enumerate() {
            if index != 0 {
                trace.push(',');
            }
            trace.push_str(&amount.to_string());
        }
    }
}

fn main() {}