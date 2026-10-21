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

fn validate_dependencies(tasks: &[Task]) -> Result<(), Error> {
    let n = tasks.len();

    for (task_id, task) in tasks.iter().enumerate() {
        let mut seen = Vec::with_capacity(task.deps.len());
        for (position, &value) in task.deps.iter().enumerate() {
            if value >= n {
                return Err(Error::BadDependency { task: task_id, position, value });
            }
            if value == task_id {
                return Err(Error::SelfDependency { task: task_id, position });
            }
            if seen.contains(&value) {
                return Err(Error::DuplicateDependency { task: task_id, position, value });
            }
            seen.push(value);
        }
    }

    Ok(())
}

fn kahn_order(tasks: &[Task]) -> Vec<usize> {
    let n = tasks.len();
    let mut indegree: Vec<usize> = tasks.iter().map(|task| task.deps.len()).collect();
    let mut children = vec![Vec::new(); n];

    for (task_id, task) in tasks.iter().enumerate() {
        for &dependency in &task.deps {
            children[dependency].push(task_id);
        }
    }

    let mut available = Vec::new();
    for (id, &degree) in indegree.iter().enumerate() {
        if degree == 0 {
            available.push(id);
        }
    }

    let mut order = Vec::with_capacity(n);
    while !available.is_empty() {
        let smallest_pos = available
            .iter()
            .enumerate()
            .min_by_key(|(_, id)| **id)
            .map(|(index, _)| index)
            .unwrap();
        let id = available.swap_remove(smallest_pos);
        order.push(id);

        for &child in &children[id] {
            indegree[child] -= 1;
            if indegree[child] == 0 {
                available.push(child);
            }
        }
    }

    order
}

fn resource_vector(used: &[u64]) -> String {
    if used.is_empty() {
        return "-".to_string();
    }

    let mut result = String::new();
    for (index, value) in used.iter().enumerate() {
        if index != 0 {
            result.push(',');
        }
        result.push_str(&value.to_string());
    }
    result
}

fn fits(task: &Task, capacity: &[u32], used: &[u64]) -> bool {
    task.needs
        .iter()
        .zip(capacity.iter().zip(used.iter()))
        .all(|(&need, (&cap, &current))| u64::from(need) <= u64::from(cap) - current)
}

fn append_event(
    trace: &mut String,
    time: u64,
    kind: &str,
    task: usize,
    worker: usize,
    used: &[u64],
) {
    trace.push_str(&time.to_string());
    trace.push('|');
    trace.push_str(kind);
    trace.push('|');
    trace.push_str(&task.to_string());
    trace.push('|');
    trace.push_str(&worker.to_string());
    trace.push('|');
    trace.push_str(&resource_vector(used));
    trace.push('\n');
}

#[derive(Clone, Copy)]
struct Running {
    task: usize,
    worker: usize,
    finish: u64,
}

pub fn schedule(
    tasks: &[Task],
    cooldowns: &[u64],
    capacity: &[u32],
) -> Result<Schedule, Error> {
    validate_dependencies(tasks)?;

    let worker_count = cooldowns.len();
    if worker_count == 0 || worker_count > 64 {
        return Err(Error::Workers { count: worker_count });
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

    let order = kahn_order(tasks);
    if order.len() != tasks.len() {
        let mut removed = vec![false; tasks.len()];
        for &id in &order {
            removed[id] = true;
        }
        let blocked = (0..tasks.len()).filter(|&id| !removed[id]).collect();
        return Err(Error::Cycle { blocked });
    }

    let mut children = vec![Vec::new(); tasks.len()];
    for (task_id, task) in tasks.iter().enumerate() {
        for &dependency in &task.deps {
            children[dependency].push(task_id);
        }
    }

    let mut tails = vec![0u64; tasks.len()];
    for &id in order.iter().rev() {
        let max_child = children[id]
            .iter()
            .map(|&child| tails[child])
            .max()
            .unwrap_or(0);
        tails[id] = tasks[id]
            .duration
            .checked_add(max_child)
            .ok_or(Error::Overflow { task: id, stage: Stage::Tail })?;
    }

    let n = tasks.len();
    let mut runs: Vec<Option<Run>> = vec![None; n];
    let mut started = vec![false; n];
    let mut completed = vec![false; n];
    let mut worker_available = vec![0u64; worker_count];
    let mut running: Vec<Running> = Vec::new();
    let mut used: Vec<u64> = vec![0; capacity.len()];
    let mut remaining_predecessors: Vec<usize> =
        tasks.iter().map(|task| task.deps.len()).collect();
    let mut trace = format!("workers={};resources={}\n", worker_count, capacity.len());
    let mut now = 0u64;
    let mut makespan = 0u64;
    let mut completed_count = 0usize;

    while completed_count < n {
        let mut ending: Vec<Running> = running
            .iter()
            .copied()
            .filter(|item| item.finish == now)
            .collect();
        ending.sort_by_key(|item| item.task);

        for item in ending {
            running.retain(|current| current.task != item.task);
            completed[item.task] = true;
            completed_count += 1;
            makespan = makespan.max(item.finish);

            for (resource, &need) in tasks[item.task].needs.iter().enumerate() {
                used[resource] -= u64::from(need);
            }
            append_event(&mut trace, now, "end", item.task, item.worker, &used);

            for &child in &children[item.task] {
                remaining_predecessors[child] -= 1;
            }
        }

        loop {
            let mut best: Option<usize> = None;

            for id in 0..n {
                if started[id]
                    || tasks[id].release > now
                    || remaining_predecessors[id] != 0
                    || !fits(&tasks[id], capacity, &used)
                {
                    continue;
                }

                let has_worker = (0..worker_count).any(|offset| {
                    let worker = (id % worker_count + offset) % worker_count;
                    worker_available[worker] <= now
                });
                if !has_worker {
                    continue;
                }

                let is_better = match best {
                    None => true,
                    Some(current) => {
                        tails[id] > tails[current]
                            || (tails[id] == tails[current]
                                && (tasks[id].priority > tasks[current].priority
                                    || (tasks[id].priority == tasks[current].priority
                                        && id < current)))
                    }
                };
                if is_better {
                    best = Some(id);
                }
            }

            let Some(id) = best else {
                break;
            };

            let worker = (0..worker_count)
                .map(|offset| (id % worker_count + offset) % worker_count)
                .find(|&worker| worker_available[worker] <= now)
                .unwrap();

            let finish = now
                .checked_add(tasks[id].duration)
                .ok_or(Error::Overflow { task: id, stage: Stage::Finish })?;
            let available_at = finish
                .checked_add(cooldowns[worker])
                .ok_or(Error::Overflow { task: id, stage: Stage::Turnaround })?;

            started[id] = true;
            runs[id] = Some(Run { start: now, finish, worker });
            worker_available[worker] = available_at;
            for (resource, &need) in tasks[id].needs.iter().enumerate() {
                used[resource] += u64::from(need);
            }

            append_event(&mut trace, now, "start", id, worker, &used);

            if finish == now {
                completed[id] = true;
                completed_count += 1;
                makespan = makespan.max(finish);
                for (resource, &need) in tasks[id].needs.iter().enumerate() {
                    used[resource] -= u64::from(need);
                }
                append_event(&mut trace, now, "end", id, worker, &used);
                for &child in &children[id] {
                    remaining_predecessors[child] -= 1;
                }
            } else {
                running.push(Running { task: id, worker, finish });
            }
        }

        if completed_count == n {
            break;
        }

        let mut next_time: Option<u64> = None;
        let mut consider = |candidate: u64| {
            if candidate > now && next_time.map_or(true, |current| candidate < current) {
                next_time = Some(candidate);
            }
        };

        for item in &running {
            consider(item.finish);
        }
        for (id, task) in tasks.iter().enumerate() {
            if !started[id] {
                consider(task.release);
            }
        }
        for &available_at in &worker_available {
            consider(available_at);
        }

        now = next_time.expect("unfinished tasks must have a future scheduling event");
    }

    let runs = runs.into_iter().map(Option::unwrap).collect();
    trace.push_str("makespan=");
    trace.push_str(&makespan.to_string());
    trace.push('\n');

    Ok(Schedule { runs, makespan, trace })
}