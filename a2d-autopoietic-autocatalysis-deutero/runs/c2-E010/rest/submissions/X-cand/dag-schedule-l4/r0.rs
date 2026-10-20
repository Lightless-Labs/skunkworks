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

fn format_used(used: &[u64]) -> String {
    let mut result = String::new();
    if used.is_empty() {
        result.push('-');
        return result;
    }
    for (index, value) in used.iter().enumerate() {
        if index > 0 {
            result.push(',');
        }
        result.push_str(&value.to_string());
    }
    result
}

fn better_task(a: usize, b: usize, tasks: &[Task], tail: &[u64]) -> bool {
    tail[a] > tail[b]
        || (tail[a] == tail[b]
            && (tasks[a].priority > tasks[b].priority
                || (tasks[a].priority == tasks[b].priority && a < b)))
}

pub fn schedule(tasks: &[Task], cooldowns: &[u64], capacity: &[u32]) -> Result<Schedule, Error> {
    let n = tasks.len();

    for (id, task) in tasks.iter().enumerate() {
        let mut seen = std::collections::BTreeSet::new();
        for (position, &dep) in task.deps.iter().enumerate() {
            if dep >= n {
                return Err(Error::BadDependency { task: id, position, value: dep });
            }
            if dep == id {
                return Err(Error::SelfDependency { task: id, position });
            }
            if !seen.insert(dep) {
                return Err(Error::DuplicateDependency { task: id, position, value: dep });
            }
        }
    }

    if !(1..=64).contains(&cooldowns.len()) {
        return Err(Error::Workers { count: cooldowns.len() });
    }

    for (id, task) in tasks.iter().enumerate() {
        if task.needs.len() != capacity.len() {
            return Err(Error::ResourceShape {
                task: id,
                expected: capacity.len(),
                actual: task.needs.len(),
            });
        }
        for (r, (&need, &cap)) in task.needs.iter().zip(capacity).enumerate() {
            if need > cap {
                return Err(Error::ResourceExceeded {
                    task: id,
                    resource: r,
                    need,
                    capacity: cap,
                });
            }
        }
    }

    let mut indegree: Vec<usize> = tasks.iter().map(|task| task.deps.len()).collect();
    let mut children = vec![Vec::new(); n];
    for (id, task) in tasks.iter().enumerate() {
        for &dep in &task.deps {
            children[dep].push(id);
        }
    }

    let mut ready = std::collections::BinaryHeap::new();
    for id in 0..n {
        if indegree[id] == 0 {
            ready.push(std::cmp::Reverse(id));
        }
    }
    let mut order = Vec::with_capacity(n);
    while let Some(std::cmp::Reverse(id)) = ready.pop() {
        order.push(id);
        for &child in &children[id] {
            indegree[child] -= 1;
            if indegree[child] == 0 {
                ready.push(std::cmp::Reverse(child));
            }
        }
    }
    if order.len() != n {
        let mut removed = vec![false; n];
        for &id in &order {
            removed[id] = true;
        }
        return Err(Error::Cycle {
            blocked: (0..n).filter(|&id| !removed[id]).collect(),
        });
    }

    let mut tail = vec![0u64; n];
    for &id in order.iter().rev() {
        let child_tail = children[id].iter().map(|&child| tail[child]).max().unwrap_or(0);
        tail[id] = tasks[id]
            .duration
            .checked_add(child_tail)
            .ok_or(Error::Overflow { task: id, stage: Stage::Tail })?;
    }

    let workers = cooldowns.len();
    let mut available_at = vec![0u64; workers];
    let mut started = vec![false; n];
    let mut remaining = tasks.iter().map(|task| task.deps.len()).collect::<Vec<_>>();
    let mut used = vec![0u64; capacity.len()];
    let mut running = Vec::<(u64, usize, usize)>::new(); // finish, task, worker
    let mut runs = vec![None; n];
    let mut trace = format!("workers={};resources={}\n", workers, capacity.len());
    let mut time = 0u64;
    let mut completed_count = 0usize;
    let mut makespan = 0u64;

    while completed_count < n {
        let mut finished_now: Vec<(usize, usize)> = running
            .iter()
            .filter(|&&(finish, _, _)| finish == time)
            .map(|&(_, task, worker)| (task, worker))
            .collect();
        finished_now.sort_unstable_by_key(|&(task, _)| task);

        for (task, worker) in finished_now {
            let index = running.iter().position(|&(_, id, _)| id == task).unwrap();
            running.swap_remove(index);
            for (r, &need) in tasks[task].needs.iter().enumerate() {
                used[r] -= u64::from(need);
            }
            completed_count += 1;
            makespan = makespan.max(time);
            trace.push_str(&format!(
                "{}|end|{}|{}|{}\n",
                time,
                task,
                worker,
                format_used(&used)
            ));
            for &child in &children[task] {
                remaining[child] -= 1;
            }
        }

        loop {
            let mut eligible = Vec::new();
            for id in 0..n {
                if started[id] || tasks[id].release > time || remaining[id] != 0 {
                    continue;
                }
                let fits = tasks[id]
                    .needs
                    .iter()
                    .enumerate()
                    .all(|(r, &need)| u64::from(need) <= u64::from(capacity[r]) - used[r]);
                if fits {
                    eligible.push(id);
                }
            }
            eligible.sort_by(|&a, &b| {
                tail[b]
                    .cmp(&tail[a])
                    .then_with(|| tasks[b].priority.cmp(&tasks[a].priority))
                    .then_with(|| a.cmp(&b))
            });

            let Some(task) = eligible.first().copied() else {
                break;
            };

            let mut chosen_worker = None;
            for offset in 0..workers {
                let worker = (task % workers + offset) % workers;
                if available_at[worker] <= time {
                    chosen_worker = Some(worker);
                    break;
                }
            }
            let Some(worker) = chosen_worker else {
                break;
            };

            let finish = time.checked_add(tasks[task].duration).ok_or(Error::Overflow {
                task,
                stage: Stage::Finish,
            })?;
            let worker_ready = finish.checked_add(cooldowns[worker]).ok_or(Error::Overflow {
                task,
                stage: Stage::Turnaround,
            })?;

            started[task] = true;
            available_at[worker] = worker_ready;
            runs[task] = Some(Run { start: time, finish, worker });
            for (r, &need) in tasks[task].needs.iter().enumerate() {
                used[r] += u64::from(need);
            }
            trace.push_str(&format!(
                "{}|start|{}|{}|{}\n",
                time,
                task,
                worker,
                format_used(&used)
            ));

            if tasks[task].duration == 0 {
                for (r, &need) in tasks[task].needs.iter().enumerate() {
                    used[r] -= u64::from(need);
                }
                completed_count += 1;
                makespan = makespan.max(time);
                trace.push_str(&format!(
                    "{}|end|{}|{}|{}\n",
                    time,
                    task,
                    worker,
                    format_used(&used)
                ));
                for &child in &children[task] {
                    remaining[child] -= 1;
                }
            } else {
                running.push((finish, task, worker));
            }
        }

        if completed_count == n {
            break;
        }

        let mut next_time: Option<u64> = None;
        let mut consider = |candidate: u64| {
            if candidate > time {
                next_time = Some(next_time.map_or(candidate, |current| current.min(candidate)));
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
        for &ready_time in &available_at {
            consider(ready_time);
        }

        match next_time {
            Some(next) => time = next,
            None => {
                return Err(Error::Cycle {
                    blocked: (0..n).filter(|&id| !started[id]).collect(),
                });
            }
        }
    }

    trace.push_str(&format!("makespan={}\n", makespan));
    Ok(Schedule {
        runs: runs.into_iter().map(Option::unwrap).collect(),
        makespan,
        trace,
    })
}
