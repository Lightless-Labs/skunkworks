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

    // Validate every dependency in input order before any other validation.
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

    // Validate resources task by task, preserving the specified error order.
    for (task_id, task) in tasks.iter().enumerate() {
        if task.needs.len() != capacity.len() {
            return Err(Error::ResourceShape {
                task: task_id,
                expected: capacity.len(),
                actual: task.needs.len(),
            });
        }
        for (resource, (&need, &limit)) in task.needs.iter().zip(capacity).enumerate() {
            if need > limit {
                return Err(Error::ResourceExceeded {
                    task: task_id,
                    resource,
                    need,
                    capacity: limit,
                });
            }
        }
    }

    // Build the graph with edges from each task to its dependents.
    let mut children = vec![Vec::<usize>::new(); n];
    let mut indegree = vec![0usize; n];
    for (task_id, task) in tasks.iter().enumerate() {
        indegree[task_id] = task.deps.len();
        for &dep in &task.deps {
            children[dep].push(task_id);
        }
    }

    // Kahn traversal, always removing the smallest currently available ID.
    let mut available = Vec::<usize>::new();
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
            .min_by_key(|&(_, id)| *id)
            .map(|(position, _)| position)
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
        let removed = {
            let mut set = vec![false; n];
            for &id in &order {
                set[id] = true;
            }
            set
        };
        let blocked = (0..n).filter(|&id| !removed[id]).collect();
        return Err(Error::Cycle { blocked });
    }

    // Compute bottom levels in reverse Kahn order.
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

    #[derive(Clone, Copy)]
    struct Running {
        task: usize,
        worker: usize,
        finish: u64,
    }

    let mut runs: Vec<Option<Run>> = vec![None; n];
    let mut started = vec![false; n];
    let mut completed = vec![false; n];
    let mut worker_available = vec![0u64; worker_count];
    let mut running = Vec::<Running>::new();
    let mut used = vec![0u64; capacity.len()];
    let mut trace = String::new();
    use std::fmt::Write as _;

    writeln!(
        &mut trace,
        "workers={};resources={}",
        worker_count,
        capacity.len()
    )
    .unwrap();

    let mut now = 0u64;
    let mut finished_count = 0usize;
    let mut makespan = 0u64;

    while finished_count < n {
        // Complete all tasks finishing now, in ascending task ID.
        let mut ending: Vec<Running> = running
            .iter()
            .copied()
            .filter(|item| item.finish == now)
            .collect();
        ending.sort_by_key(|item| item.task);

        if !ending.is_empty() {
            let ending_ids: Vec<usize> = ending.iter().map(|item| item.task).collect();
            for item in &ending {
                for (r, &need) in tasks[item.task].needs.iter().enumerate() {
                    used[r] -= u64::from(need);
                }
                completed[item.task] = true;
                finished_count += 1;
                makespan = makespan.max(item.finish);

                trace_event(&mut trace, now, "end", item.task, item.worker, &used);
            }
            running.retain(|item| !ending_ids.contains(&item.task));
        }

        // Start tasks one at a time, reevaluating eligibility and resources
        // after each admission and each immediate zero-duration completion.
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

            // Choose the first available worker in the task's cyclic preference.
            let preferred = task_id % worker_count;
            let worker = (0..worker_count)
                .map(|offset| (preferred + offset) % worker_count)
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
            for (r, &need) in tasks[task_id].needs.iter().enumerate() {
                used[r] += u64::from(need);
            }
            runs[task_id] = Some(Run {
                start: now,
                finish,
                worker,
            });

            trace_event(&mut trace, now, "start", task_id, worker, &used);

            if finish == now {
                // Immediate completion precedes selection of another start.
                for (r, &need) in tasks[task_id].needs.iter().enumerate() {
                    used[r] -= u64::from(need);
                }
                completed[task_id] = true;
                finished_count += 1;
                makespan = makespan.max(finish);
                trace_event(&mut trace, now, "end", task_id, worker, &used);
            } else {
                running.push(Running {
                    task: task_id,
                    worker,
                    finish,
                });
            }
        }

        if finished_count == n {
            break;
        }

        // Advance to the earliest event that can change eligibility or availability.
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
        for &available_at in &worker_available {
            if available_at > now {
                next_time = Some(next_time.map_or(available_at, |t| t.min(available_at)));
            }
        }

        // If no future event exists, no unstarted task can be schedulable.
        // With an acyclic graph, this is only possible for the empty input,
        // already handled by the loop condition.
        let Some(next) = next_time else {
            break;
        };
        now = next;
    }

    writeln!(&mut trace, "makespan={}", makespan).unwrap();

    let runs = runs
        .into_iter()
        .map(|run| run.expect("acyclic tasks are all scheduled"))
        .collect();

    Ok(Schedule {
        runs,
        makespan,
        trace,
    })
}

fn trace_event(
    trace: &mut String,
    time: u64,
    event: &str,
    task: usize,
    worker: usize,
    used: &[u64],
) {
    use std::fmt::Write as _;

    write!(trace, "{}|{}|{}|{}|", time, event, task, worker).unwrap();
    if used.is_empty() {
        trace.push('-');
    } else {
        for (index, amount) in used.iter().enumerate() {
            if index != 0 {
                trace.push(',');
            }
            write!(trace, "{}", amount).unwrap();
        }
    }
    trace.push('\n');
}