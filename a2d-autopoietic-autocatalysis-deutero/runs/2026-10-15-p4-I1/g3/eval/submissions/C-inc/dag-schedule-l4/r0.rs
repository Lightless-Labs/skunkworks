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

    let mut children = vec![Vec::new(); n];
    let mut indegree = vec![0usize; n];
    for (task_id, task) in tasks.iter().enumerate() {
        indegree[task_id] = task.deps.len();
        for &predecessor in &task.deps {
            children[predecessor].push(task_id);
        }
    }

    let mut available: Vec<usize> = (0..n).filter(|&i| indegree[i] == 0).collect();
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
            indegree[child] -= 1;
            if indegree[child] == 0 {
                available.push(child);
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
    let mut used = vec![0u64; resource_count];
    let mut started = vec![false; n];
    let mut completed = vec![false; n];
    let mut remaining_predecessors: Vec<usize> =
        tasks.iter().map(|task| task.deps.len()).collect();
    let mut worker_available = vec![0u64; worker_count];
    let mut running: Vec<Option<(u64, usize)>> = vec![None; worker_count];
    let mut runs = vec![
        Run {
            start: 0,
            finish: 0,
            worker: 0,
        };
        n
    ];
    let mut trace = String::new();
    trace.push_str(&format!(
        "workers={};resources={}\n",
        worker_count, resource_count
    ));

    let mut time = 0u64;
    let mut finished_count = 0usize;
    let mut makespan = 0u64;

    while finished_count < n {
        let mut ending: Vec<(usize, usize)> = running
            .iter()
            .enumerate()
            .filter_map(|(worker, entry)| match entry {
                Some((finish, task_id)) if *finish == time => Some((*task_id, worker)),
                _ => None,
            })
            .collect();
        ending.sort_by_key(|&(task_id, _)| task_id);

        for (task_id, worker) in ending {
            running[worker] = None;
            completed[task_id] = true;
            finished_count += 1;
            makespan = makespan.max(time);

            for (resource, &need) in tasks[task_id].needs.iter().enumerate() {
                used[resource] -= u64::from(need);
            }

            trace.push_str(&format!("{}|end|{}|{}|", time, task_id, worker));
            append_usage(&mut trace, &used);
            trace.push('\n');

            for &child in &children[task_id] {
                remaining_predecessors[child] -= 1;
            }
        }

        loop {
            let task_choice = (0..n)
                .filter(|&id| {
                    !started[id]
                        && tasks[id].release <= time
                        && remaining_predecessors[id] == 0
                        && tasks[id]
                            .needs
                            .iter()
                            .enumerate()
                            .all(|(r, &need)| u64::from(need) <= u64::from(capacity[r]) - used[r])
                })
                .max_by(|&a, &b| {
                    tails[a]
                        .cmp(&tails[b])
                        .then_with(|| tasks[a].priority.cmp(&tasks[b].priority))
                        .then_with(|| b.cmp(&a))
                });

            let Some(task_id) = task_choice else {
                break;
            };

            let worker = (0..worker_count)
                .map(|offset| (task_id % worker_count + offset) % worker_count)
                .find(|&worker| worker_available[worker] <= time);
            let Some(worker) = worker else {
                break;
            };

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
            runs[task_id] = Run {
                start: time,
                finish,
                worker,
            };

            for (resource, &need) in tasks[task_id].needs.iter().enumerate() {
                used[resource] += u64::from(need);
            }

            trace.push_str(&format!("{}|start|{}|{}|", time, task_id, worker));
            append_usage(&mut trace, &used);
            trace.push('\n');

            if finish == time {
                completed[task_id] = true;
                finished_count += 1;
                makespan = makespan.max(time);

                for (resource, &need) in tasks[task_id].needs.iter().enumerate() {
                    used[resource] -= u64::from(need);
                }

                trace.push_str(&format!("{}|end|{}|{}|", time, task_id, worker));
                append_usage(&mut trace, &used);
                trace.push('\n');

                for &child in &children[task_id] {
                    remaining_predecessors[child] -= 1;
                }
            } else {
                running[worker] = Some((finish, task_id));
            }
        }

        if finished_count == n {
            break;
        }

        let mut next_time: Option<u64> = None;
        for entry in &running {
            if let Some((finish, _)) = entry {
                if *finish > time {
                    next_time = Some(next_time.map_or(*finish, |current| current.min(*finish)));
                }
            }
        }
        for id in 0..n {
            if !started[id] && tasks[id].release > time {
                next_time = Some(next_time.map_or(tasks[id].release, |current| {
                    current.min(tasks[id].release)
                }));
            }
        }
        for &available_at in &worker_available {
            if available_at > time {
                next_time = Some(next_time.map_or(available_at, |current| current.min(available_at)));
            }
        }

        time = next_time.expect("unfinished tasks must have a future event");
    }

    trace.push_str(&format!("makespan={}\n", makespan));
    Ok(Schedule {
        runs,
        makespan,
        trace,
    })
}

fn append_usage(output: &mut String, used: &[u64]) {
    if used.is_empty() {
        output.push('-');
        return;
    }
    for (index, amount) in used.iter().enumerate() {
        if index > 0 {
            output.push(',');
        }
        output.push_str(&amount.to_string());
    }
}

fn main() {}