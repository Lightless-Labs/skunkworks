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

    // Dependency validation takes precedence over every other check.
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

    let worker_count = cooldowns.len();
    if !(1..=64).contains(&worker_count) {
        return Err(Error::Workers {
            count: worker_count,
        });
    }

    // Resource shape and demand validation is task-major.
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

    // Build successor lists and indegrees.
    let mut children = vec![Vec::<usize>::new(); n];
    let mut indegree = vec![0usize; n];
    for (task_id, task) in tasks.iter().enumerate() {
        indegree[task_id] = task.deps.len();
        for &pred in &task.deps {
            children[pred].push(task_id);
        }
    }

    // Deterministic Kahn traversal: always remove the smallest available ID.
    let mut available = BTreeSet::new();
    for (task_id, &degree) in indegree.iter().enumerate() {
        if degree == 0 {
            available.insert(task_id);
        }
    }
    let mut order = Vec::with_capacity(n);
    while let Some(task_id) = available.pop_first() {
        order.push(task_id);
        for &child in &children[task_id] {
            indegree[child] -= 1;
            if indegree[child] == 0 {
                available.insert(child);
            }
        }
    }

    if order.len() != n {
        let removed: Vec<bool> = {
            let mut v = vec![false; n];
            for &id in &order {
                v[id] = true;
            }
            v
        };
        return Err(Error::Cycle {
            blocked: (0..n).filter(|&id| !removed[id]).collect(),
        });
    }

    // Bottom levels, in reverse topological order.
    let mut tail = vec![0u64; n];
    for &task_id in order.iter().rev() {
        let max_child = children[task_id]
            .iter()
            .map(|&child| tail[child])
            .max()
            .unwrap_or(0);
        tail[task_id] = tasks[task_id]
            .duration
            .checked_add(max_child)
            .ok_or(Error::Overflow {
                task: task_id,
                stage: Stage::Tail,
            })?;
    }

    let mut trace = format!("workers={worker_count};resources={}\n", capacity.len());
    let mut runs: Vec<Option<Run>> = vec![None; n];
    let mut started = vec![false; n];
    let mut completed = vec![false; n];
    let mut remaining_preds: Vec<usize> = tasks.iter().map(|t| t.deps.len()).collect();
    let mut worker_available = vec![0u64; worker_count];
    let mut used = vec![0u64; capacity.len()];

    #[derive(Clone)]
    struct Running {
        task: usize,
        worker: usize,
        finish: u64,
        turnaround: u64,
    }

    let mut running: Vec<Running> = Vec::new();
    let mut now = 0u64;
    let mut finished_count = 0usize;
    let mut makespan = 0u64;

    while finished_count < n {
        // Complete all tasks finishing now, in task-ID order.
        let mut ending: Vec<usize> = running
            .iter()
            .enumerate()
            .filter_map(|(idx, item)| (item.finish == now).then_some(idx))
            .collect();
        ending.sort_by_key(|&idx| running[idx].task);

        for idx in ending.into_iter().rev() {
            let item = running.remove(idx);
            let task_id = item.task;
            for (r, need) in tasks[task_id].needs.iter().enumerate() {
                used[r] -= u64::from(*need);
            }
            completed[task_id] = true;
            finished_count += 1;
            makespan = makespan.max(item.finish);
            for &child in &children[task_id] {
                remaining_preds[child] -= 1;
            }

            trace.push_str(&format!("{now}|end|{task_id}|{}|", item.worker));
            append_used(&mut trace, &used);
            trace.push('\n');
        }

        loop {
            let mut best: Option<usize> = None;

            for task_id in 0..n {
                if started[task_id]
                    || tasks[task_id].release > now
                    || remaining_preds[task_id] != 0
                {
                    continue;
                }

                let fits = tasks[task_id]
                    .needs
                    .iter()
                    .enumerate()
                    .all(|(r, &need)| {
                        u64::from(need) <= u64::from(capacity[r]) - used[r]
                    });
                if !fits {
                    continue;
                }

                let better = match best {
                    None => true,
                    Some(current) => {
                        tail[task_id] > tail[current]
                            || (tail[task_id] == tail[current]
                                && (tasks[task_id].priority > tasks[current].priority
                                    || (tasks[task_id].priority == tasks[current].priority
                                        && task_id < current)))
                    }
                };
                if better {
                    best = Some(task_id);
                }
            }

            let Some(task_id) = best else {
                break;
            };

            // Select the task first, then choose its preferred available worker.
            let preferred = task_id % worker_count;
            let worker = (0..worker_count)
                .map(|offset| (preferred + offset) % worker_count)
                .find(|&w| worker_available[w] <= now);

            let Some(worker) = worker else {
                break;
            };

            let finish = now.checked_add(tasks[task_id].duration).ok_or(Error::Overflow {
                task: task_id,
                stage: Stage::Finish,
            })?;
            let turnaround = finish.checked_add(cooldowns[worker]).ok_or(Error::Overflow {
                task: task_id,
                stage: Stage::Turnaround,
            })?;

            started[task_id] = true;
            worker_available[worker] = turnaround;
            for (r, need) in tasks[task_id].needs.iter().enumerate() {
                used[r] += u64::from(*need);
            }
            runs[task_id] = Some(Run {
                start: now,
                finish,
                worker,
            });

            trace.push_str(&format!("{now}|start|{task_id}|{worker}|"));
            append_used(&mut trace, &used);
            trace.push('\n');

            if finish == now {
                // Immediate completion of a newly started zero-duration task.
                for (r, need) in tasks[task_id].needs.iter().enumerate() {
                    used[r] -= u64::from(*need);
                }
                completed[task_id] = true;
                finished_count += 1;
                makespan = makespan.max(finish);
                for &child in &children[task_id] {
                    remaining_preds[child] -= 1;
                }

                trace.push_str(&format!("{now}|end|{task_id}|{worker}|"));
                append_used(&mut trace, &used);
                trace.push('\n');
            } else {
                running.push(Running {
                    task: task_id,
                    worker,
                    finish,
                    turnaround,
                });
            }
        }

        if finished_count == n {
            break;
        }

        // Advance to the earliest event that can change admission or completion.
        let next_finish = running.iter().map(|item| item.finish).min();
        let next_release = (0..n)
            .filter(|&id| !started[id] && tasks[id].release > now)
            .map(|id| tasks[id].release)
            .min();
        let next_worker = worker_available
            .iter()
            .copied()
            .filter(|&time| time > now)
            .min();

        let next = [next_finish, next_release, next_worker]
            .into_iter()
            .flatten()
            .min();

        match next {
            Some(time) => now = time,
            None => break,
        }
    }

    trace.push_str(&format!("makespan={makespan}\n"));

    Ok(Schedule {
        runs: runs.into_iter().map(Option::unwrap).collect(),
        makespan,
        trace,
    })
}

fn append_used(trace: &mut String, used: &[u64]) {
    if used.is_empty() {
        trace.push('-');
        return;
    }
    for (index, value) in used.iter().enumerate() {
        if index != 0 {
            trace.push(',');
        }
        trace.push_str(&value.to_string());
    }
}