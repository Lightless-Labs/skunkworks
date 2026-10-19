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

pub fn schedule(
    tasks: &[Task],
    cooldowns: &[u64],
    capacity: &[u32],
) -> Result<Schedule, Error> {
    use std::cmp::Reverse;
    use std::collections::BinaryHeap;

    let n = tasks.len();

    // Validate every dependency entry before any other checks.
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
        for (resource, &need) in task.needs.iter().enumerate() {
            if need > capacity[resource] {
                return Err(Error::ResourceExceeded {
                    task: task_id,
                    resource,
                    need,
                    capacity: capacity[resource],
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

    // Kahn traversal with the smallest currently available ID first.
    let mut available = BinaryHeap::new();
    for (task_id, &degree) in indegree.iter().enumerate() {
        if degree == 0 {
            available.push(Reverse(task_id));
        }
    }

    let mut order = Vec::with_capacity(n);
    let mut removed = vec![false; n];
    while let Some(Reverse(task_id)) = available.pop() {
        removed[task_id] = true;
        order.push(task_id);
        for &child in &children[task_id] {
            indegree[child] -= 1;
            if indegree[child] == 0 {
                available.push(Reverse(child));
            }
        }
    }

    if order.len() != n {
        let blocked = (0..n).filter(|&task_id| !removed[task_id]).collect();
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

    let mut runs: Vec<Option<Run>> = vec![None; n];
    let mut started = vec![false; n];
    let mut completed = vec![false; n];
    let mut running = Vec::<(u64, usize, usize)>::new(); // (finish, task, worker)
    let mut worker_available = vec![0u64; worker_count];
    let mut used = vec![0u64; capacity.len()];
    let mut trace = format!("workers={worker_count};resources={}\n", capacity.len());
    let mut time = 0u64;
    let mut makespan = 0u64;
    let mut completed_count = 0usize;

    fn append_usage(trace: &mut String, used: &[u64]) {
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

    while completed_count < n {
        // Complete all tasks finishing now, in ascending task ID order.
        let mut ending: Vec<(u64, usize, usize)> = running
            .iter()
            .copied()
            .filter(|(finish, _, _)| *finish == time)
            .collect();
        ending.sort_by_key(|&(_, task_id, _)| task_id);

        if !ending.is_empty() {
            running.retain(|(finish, _, _)| *finish != time);
            for (finish, task_id, worker) in ending {
                completed[task_id] = true;
                completed_count += 1;
                makespan = makespan.max(finish);
                for resource in 0..capacity.len() {
                    used[resource] -= u64::from(tasks[task_id].needs[resource]);
                }
                trace.push_str(&format!("{time}|end|{task_id}|{worker}|"));
                append_usage(&mut trace, &used);
                trace.push('\n');
            }
        }

        // Repeatedly admit one fitting task, then reevaluate.
        loop {
            let worker = (0..worker_count)
                .find(|&w| worker_available[w] <= time);
            let Some(_) = worker else {
                break;
            };

            let mut candidates = Vec::new();
            for task_id in 0..n {
                if started[task_id] || tasks[task_id].release > time {
                    continue;
                }
                if !tasks[task_id]
                    .deps
                    .iter()
                    .all(|&dep| completed[dep])
                {
                    continue;
                }
                let fits = (0..capacity.len()).all(|resource| {
                    u64::from(tasks[task_id].needs[resource])
                        <= u64::from(capacity[resource]) - used[resource]
                });
                if fits {
                    candidates.push(task_id);
                }
            }

            candidates.sort_by(|&a, &b| {
                tails[b]
                    .cmp(&tails[a])
                    .then_with(|| tasks[b].priority.cmp(&tasks[a].priority))
                    .then_with(|| a.cmp(&b))
            });
            let Some(task_id) = candidates.first().copied() else {
                break;
            };

            // Select a worker only after selecting the task.
            let preferred = task_id % worker_count;
            let worker = (0..worker_count)
                .map(|offset| (preferred + offset) % worker_count)
                .find(|&w| worker_available[w] <= time)
                .expect("a worker was established as available");

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

            started[task_id] = true;
            worker_available[worker] = available_at;
            for resource in 0..capacity.len() {
                used[resource] += u64::from(tasks[task_id].needs[resource]);
            }
            runs[task_id] = Some(Run {
                start: time,
                finish,
                worker,
            });

            trace.push_str(&format!("{time}|start|{task_id}|{worker}|"));
            append_usage(&mut trace, &used);
            trace.push('\n');

            if finish == time {
                completed[task_id] = true;
                completed_count += 1;
                makespan = makespan.max(finish);
                for resource in 0..capacity.len() {
                    used[resource] -= u64::from(tasks[task_id].needs[resource]);
                }
                trace.push_str(&format!("{time}|end|{task_id}|{worker}|"));
                append_usage(&mut trace, &used);
                trace.push('\n');
            } else {
                running.push((finish, task_id, worker));
            }
        }

        if completed_count == n {
            break;
        }

        // Move to the earliest strictly later event that can change the state.
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
            if !started[task_id] {
                consider(tasks[task_id].release);
            }
        }
        for &available_at in &worker_available {
            consider(available_at);
        }

        time = next_time.expect("unfinished tasks always have a future event");
    }

    trace.push_str(&format!("makespan={makespan}\n"));

    Ok(Schedule {
        runs: runs.into_iter().map(Option::unwrap).collect(),
        makespan,
        trace,
    })
}