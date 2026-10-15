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

    // Validate dependencies in the required order.
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

    if !(1..=64).contains(&cooldowns.len()) {
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

    // Build graph edges from each task to its dependents.
    let mut children = vec![Vec::<usize>::new(); n];
    let mut indegree = vec![0usize; n];
    for (task_id, task) in tasks.iter().enumerate() {
        indegree[task_id] = task.deps.len();
        for &dep in &task.deps {
            children[dep].push(task_id);
        }
    }

    // Kahn traversal, always removing the smallest available task ID.
    let mut available = vec![false; n];
    for i in 0..n {
        if indegree[i] == 0 {
            available[i] = true;
        }
    }

    let mut order = Vec::with_capacity(n);
    while let Some(id) = available.iter().position(|&is_available| is_available) {
        available[id] = false;
        order.push(id);
        for &child in &children[id] {
            indegree[child] -= 1;
            if indegree[child] == 0 {
                available[child] = true;
            }
        }
    }

    if order.len() != n {
        let removed = {
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

    // Compute bottom-level tails in reverse Kahn order.
    let mut tails = vec![0u64; n];
    for &id in order.iter().rev() {
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

    #[derive(Clone)]
    struct Running {
        task: usize,
        worker: usize,
        finish: u64,
    }

    let worker_count = cooldowns.len();
    let mut worker_available = vec![0u64; worker_count];
    let mut started = vec![false; n];
    let mut completed = vec![false; n];
    let mut used = vec![0u64; capacity.len()];
    let mut running = Vec::<Running>::new();
    let mut runs: Vec<Option<Run>> = vec![None; n];
    let mut trace = format!("workers={};resources={}\n", worker_count, capacity.len());
    let mut now = 0u64;
    let mut makespan = 0u64;

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

    loop {
        // Complete all tasks finishing now in ascending task ID order.
        let mut finishing: Vec<usize> = running
            .iter()
            .enumerate()
            .filter_map(|(idx, r)| (r.finish == now).then_some(idx))
            .collect();
        finishing.sort_by_key(|&idx| running[idx].task);

        for idx in finishing.into_iter().rev() {
            let r = running.remove(idx);
            completed[r.task] = true;
            for (resource, &need) in tasks[r.task].needs.iter().enumerate() {
                used[resource] -= u64::from(need);
            }
            trace.push_str(&format!("{}|end|{}|{}|", now, r.task, r.worker));
            append_resources(&mut trace, &used);
            trace.push('\n');
        }

        // Admit as many tasks as possible, reevaluating after each start.
        loop {
            let mut candidates: Vec<usize> = (0..n)
                .filter(|&id| {
                    if started[id] || tasks[id].release > now {
                        return false;
                    }
                    if !tasks[id].deps.iter().all(|&dep| completed[dep]) {
                        return false;
                    }
                    tasks[id]
                        .needs
                        .iter()
                        .enumerate()
                        .all(|(r, &need)| u64::from(need) <= u64::from(capacity[r]) - used[r])
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

            let worker = (0..worker_count)
                .map(|offset| (task_id % worker_count + offset) % worker_count)
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
            for (resource, &need) in tasks[task_id].needs.iter().enumerate() {
                used[resource] += u64::from(need);
            }
            runs[task_id] = Some(Run {
                start: now,
                finish,
                worker,
            });
            trace.push_str(&format!("{}|start|{}|{}|", now, task_id, worker));
            append_resources(&mut trace, &used);
            trace.push('\n');

            if tasks[task_id].duration == 0 {
                completed[task_id] = true;
                for (resource, &need) in tasks[task_id].needs.iter().enumerate() {
                    used[resource] -= u64::from(need);
                }
                trace.push_str(&format!("{}|end|{}|{}|", now, task_id, worker));
                append_resources(&mut trace, &used);
                trace.push('\n');
                makespan = makespan.max(now);
            } else {
                running.push(Running {
                    task: task_id,
                    worker,
                    finish,
                });
                makespan = makespan.max(finish);
            }
        }

        if completed.iter().all(|&done| done) {
            break;
        }

        // Advance to the next time at which a state change can occur.
        let mut next = None::<u64>;
        let mut consider = |time: u64| {
            if time > now {
                next = Some(next.map_or(time, |old| old.min(time)));
            }
        };

        for r in &running {
            consider(r.finish);
        }
        for id in 0..n {
            if !started[id] {
                consider(tasks[id].release);
            }
        }
        for &available_at in &worker_available {
            consider(available_at);
        }

        now = next.expect("unfinished tasks must have a future event");
    }

    trace.push_str(&format!("makespan={}\n", makespan));

    Ok(Schedule {
        runs: runs.into_iter().map(Option::unwrap).collect(),
        makespan,
        trace,
    })
}

fn main() {}