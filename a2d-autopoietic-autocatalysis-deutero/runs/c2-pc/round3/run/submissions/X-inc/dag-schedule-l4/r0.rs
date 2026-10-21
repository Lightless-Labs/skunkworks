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

    // Validate every dependency before performing any other validation.
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
        for resource in 0..capacity.len() {
            if task.needs[resource] > capacity[resource] {
                return Err(Error::ResourceExceeded {
                    task: task_id,
                    resource,
                    need: task.needs[resource],
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
        for &pred in &task.deps {
            children[pred].push(task_id);
        }
    }

    // Kahn traversal, always removing the smallest currently available ID.
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
        let mut removed = vec![false; n];
        for &id in &topo {
            removed[id] = true;
        }
        return Err(Error::Cycle {
            blocked: (0..n).filter(|&id| !removed[id]).collect(),
        });
    }

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

    #[derive(Clone, Copy)]
    struct Running {
        task: usize,
        worker: usize,
        finish: u64,
    }

    let worker_count = cooldowns.len();
    let mut runs: Vec<Option<Run>> = vec![None; n];
    let mut completed = vec![false; n];
    let mut started = vec![false; n];
    let mut used = vec![0u64; capacity.len()];
    let mut worker_free = vec![0u64; worker_count];
    let mut running = Vec::<Running>::new();
    let mut trace = format!("workers={};resources={}\n", worker_count, capacity.len());

    fn append_resources(trace: &mut String, used: &[u64]) {
        if used.is_empty() {
            trace.push('-');
        } else {
            for (i, value) in used.iter().enumerate() {
                if i != 0 {
                    trace.push(',');
                }
                trace.push_str(&value.to_string());
            }
        }
    }

    let mut now = 0u64;
    let mut makespan = 0u64;
    let mut completed_count = 0usize;

    while completed_count < n {
        // Complete every task finishing now, in ascending task ID order.
        let mut finishing: Vec<Running> = running
            .iter()
            .copied()
            .filter(|item| item.finish == now)
            .collect();
        finishing.sort_by_key(|item| item.task);

        for item in finishing {
            for (r, &need) in tasks[item.task].needs.iter().enumerate() {
                used[r] -= u64::from(need);
            }
            completed[item.task] = true;
            completed_count += 1;
            trace.push_str(&format!(
                "{}|end|{}|{}|",
                now, item.task, item.worker
            ));
            append_resources(&mut trace, &used);
            trace.push('\n');
            running.retain(|other| other.task != item.task);
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
                let fits = tasks[id]
                    .needs
                    .iter()
                    .enumerate()
                    .all(|(r, &need)| u64::from(need) <= u64::from(capacity[r]) - used[r]);
                if !fits {
                    continue;
                }
                match best {
                    None => best = Some(id),
                    Some(current) => {
                        let a = (tails[id], tasks[id].priority, std::cmp::Reverse(id));
                        let b = (
                            tails[current],
                            tasks[current].priority,
                            std::cmp::Reverse(current),
                        );
                        if a > b {
                            best = Some(id);
                        }
                    }
                }
            }

            let Some(id) = best else {
                break;
            };

            let preferred = id % worker_count;
            let worker = (0..worker_count)
                .map(|offset| (preferred + offset) % worker_count)
                .find(|&w| worker_free[w] <= now);
            let Some(worker) = worker else {
                break;
            };

            let finish = now
                .checked_add(tasks[id].duration)
                .ok_or(Error::Overflow {
                    task: id,
                    stage: Stage::Finish,
                })?;
            let free_at = finish
                .checked_add(cooldowns[worker])
                .ok_or(Error::Overflow {
                    task: id,
                    stage: Stage::Turnaround,
                })?;

            started[id] = true;
            worker_free[worker] = free_at;
            for (r, &need) in tasks[id].needs.iter().enumerate() {
                used[r] += u64::from(need);
            }
            runs[id] = Some(Run {
                start: now,
                finish,
                worker,
            });
            running.push(Running {
                task: id,
                worker,
                finish,
            });

            trace.push_str(&format!("{}|start|{}|{}|", now, id, worker));
            append_resources(&mut trace, &used);
            trace.push('\n');

            if finish > makespan {
                makespan = finish;
            }

            // Zero-duration tasks complete immediately before another admission.
            if finish == now {
                for (r, &need) in tasks[id].needs.iter().enumerate() {
                    used[r] -= u64::from(need);
                }
                completed[id] = true;
                completed_count += 1;
                trace.push_str(&format!("{}|end|{}|{}|", now, id, worker));
                append_resources(&mut trace, &used);
                trace.push('\n');
                running.retain(|item| item.task != id);
            }
        }

        if completed_count == n {
            break;
        }

        let mut next = None::<u64>;
        for item in &running {
            if item.finish > now {
                next = Some(next.map_or(item.finish, |x| x.min(item.finish)));
            }
        }
        for id in 0..n {
            if !started[id] && tasks[id].release > now {
                next = Some(next.map_or(tasks[id].release, |x| x.min(tasks[id].release)));
            }
        }
        for &free_at in &worker_free {
            if free_at > now {
                next = Some(next.map_or(free_at, |x| x.min(free_at)));
            }
        }

        match next {
            Some(time) => now = time,
            None => {
                // With an acyclic graph, valid workers, and finite releases,
                // this can only occur if all tasks have already completed.
                break;
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