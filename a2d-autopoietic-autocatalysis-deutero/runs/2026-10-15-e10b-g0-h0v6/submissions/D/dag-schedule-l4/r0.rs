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

#[derive(Clone)]
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
    let n = tasks.len();

    // Validate every dependency before any other validation.
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
    let mut indegree = vec![0usize; n];
    let mut children = vec![Vec::<usize>::new(); n];
    for (task_id, task) in tasks.iter().enumerate() {
        indegree[task_id] = task.deps.len();
        for &dep in &task.deps {
            children[dep].push(task_id);
        }
    }

    // Kahn traversal, always removing the smallest available task ID.
    let mut available = Vec::<usize>::new();
    for id in 0..n {
        if indegree[id] == 0 {
            available.push(id);
        }
    }
    let mut order = Vec::with_capacity(n);
    let mut remaining_indegree = indegree.clone();
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
            remaining_indegree[child] -= 1;
            if remaining_indegree[child] == 0 {
                available.push(child);
            }
        }
    }

    if order.len() != n {
        let removed = {
            let mut removed = vec![false; n];
            for &id in &order {
                removed[id] = true;
            }
            removed
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

    let mut trace = String::new();
    use std::fmt::Write as _;
    writeln!(
        &mut trace,
        "workers={};resources={}",
        worker_count,
        capacity.len()
    )
    .unwrap();

    let mut used = vec![0u64; capacity.len()];
    let mut completed = vec![false; n];
    let mut started = vec![false; n];
    let mut worker_available_at = vec![0u64; worker_count];
    let mut running = Vec::<Running>::new();
    let mut runs = vec![
        Run {
            start: 0,
            finish: 0,
            worker: 0
        };
        n
    ];
    let mut current_time = 0u64;
    let mut completed_count = 0usize;
    let mut makespan = 0u64;

    fn write_resources(trace: &mut String, used: &[u64]) {
        use std::fmt::Write as _;
        if used.is_empty() {
            trace.push('-');
        } else {
            for (i, value) in used.iter().enumerate() {
                if i > 0 {
                    trace.push(',');
                }
                write!(trace, "{value}").unwrap();
            }
        }
    }

    while completed_count < n {
        // Complete all tasks finishing now, in task ID order.
        let mut finishing: Vec<usize> = running
            .iter()
            .enumerate()
            .filter(|(_, r)| r.finish == current_time)
            .map(|(i, _)| i)
            .collect();
        finishing.sort_by_key(|&i| running[i].task);

        for index in finishing.into_iter().rev() {
            let r = running.remove(index);
            completed[r.task] = true;
            completed_count += 1;
            for (resource, &need) in tasks[r.task].needs.iter().enumerate() {
                used[resource] -= u64::from(need);
            }
            write!(&mut trace, "{}|end|{}|{}|", current_time, r.task, r.worker).unwrap();
            write_resources(&mut trace, &used);
            trace.push('\n');
        }

        // Repeatedly admit one task, reevaluating eligibility and priority each time.
        loop {
            let free_worker_exists = worker_available_at
                .iter()
                .any(|&available_at| available_at <= current_time);
            if !free_worker_exists {
                break;
            }

            let mut candidates: Vec<usize> = (0..n)
                .filter(|&id| {
                    if started[id] || tasks[id].release > current_time {
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

            if candidates.is_empty() {
                break;
            }

            candidates.sort_by(|&a, &b| {
                tails[b]
                    .cmp(&tails[a])
                    .then_with(|| tasks[b].priority.cmp(&tasks[a].priority))
                    .then_with(|| a.cmp(&b))
            });
            let task_id = candidates[0];

            let preferred = task_id % worker_count;
            let worker = (0..worker_count)
                .map(|offset| (preferred + offset) % worker_count)
                .find(|&w| worker_available_at[w] <= current_time)
                .unwrap();

            let finish = current_time
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
            worker_available_at[worker] = available_at;
            for (resource, &need) in tasks[task_id].needs.iter().enumerate() {
                used[resource] += u64::from(need);
            }
            runs[task_id] = Run {
                start: current_time,
                finish,
                worker,
            };
            makespan = makespan.max(finish);

            write!(
                &mut trace,
                "{}|start|{}|{}|",
                current_time, task_id, worker
            )
            .unwrap();
            write_resources(&mut trace, &used);
            trace.push('\n');

            if finish == current_time {
                // Complete a newly started zero-duration task immediately.
                completed[task_id] = true;
                completed_count += 1;
                for (resource, &need) in tasks[task_id].needs.iter().enumerate() {
                    used[resource] -= u64::from(need);
                }
                write!(
                    &mut trace,
                    "{}|end|{}|{}|",
                    current_time, task_id, worker
                )
                .unwrap();
                write_resources(&mut trace, &used);
                trace.push('\n');
            } else {
                running.push(Running {
                    task: task_id,
                    worker,
                    finish,
                });
            }
        }

        if completed_count == n {
            break;
        }

        let mut next_time: Option<u64> = None;
        for r in &running {
            if r.finish > current_time {
                next_time = Some(next_time.map_or(r.finish, |t| t.min(r.finish)));
            }
        }
        for id in 0..n {
            if !started[id] && tasks[id].release > current_time {
                let t = tasks[id].release;
                next_time = Some(next_time.map_or(t, |x| x.min(t)));
            }
        }
        for &available_at in &worker_available_at {
            if available_at > current_time {
                next_time = Some(next_time.map_or(available_at, |t| t.min(available_at)));
            }
        }

        // If no task can run and no event can change eligibility, all remaining
        // tasks are necessarily waiting on a running predecessor or future event.
        if let Some(t) = next_time {
            current_time = t;
        } else {
            // This state is unreachable for an acyclic graph; keep the function
            // total even if invariants are changed in the future.
            break;
        }
    }

    writeln!(&mut trace, "makespan={makespan}").unwrap();
    Ok(Schedule {
        runs,
        makespan,
        trace,
    })
}