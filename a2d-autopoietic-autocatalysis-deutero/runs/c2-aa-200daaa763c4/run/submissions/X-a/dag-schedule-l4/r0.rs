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
    let n = tasks.len();

    // Validate all dependency entries before all other work.
    for (task_id, task) in tasks.iter().enumerate() {
        let mut seen = std::collections::HashSet::new();
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

    // Build the forward graph and perform deterministic Kahn traversal.
    let mut children = vec![Vec::<usize>::new(); n];
    let mut indegree = vec![0usize; n];
    for (task_id, task) in tasks.iter().enumerate() {
        indegree[task_id] = task.deps.len();
        for &dep in &task.deps {
            children[dep].push(task_id);
        }
    }

    let mut available = std::collections::BTreeSet::new();
    for (id, &degree) in indegree.iter().enumerate() {
        if degree == 0 {
            available.insert(id);
        }
    }

    let mut order = Vec::with_capacity(n);
    while let Some(&id) = available.iter().next() {
        available.remove(&id);
        order.push(id);
        for &child in &children[id] {
            indegree[child] -= 1;
            if indegree[child] == 0 {
                available.insert(child);
            }
        }
    }

    if order.len() != n {
        let removed = order.iter().copied().collect::<std::collections::HashSet<_>>();
        return Err(Error::Cycle {
            blocked: (0..n).filter(|id| !removed.contains(id)).collect(),
        });
    }

    let mut tails = vec![0u64; n];
    for &id in order.iter().rev() {
        let successor_tail = children[id]
            .iter()
            .map(|&child| tails[child])
            .max()
            .unwrap_or(0);
        tails[id] = tasks[id]
            .duration
            .checked_add(successor_tail)
            .ok_or(Error::Overflow {
                task: id,
                stage: Stage::Tail,
            })?;
    }

    let worker_count = cooldowns.len();
    let mut worker_free = vec![0u64; worker_count];
    let mut started = vec![false; n];
    let mut completed = vec![false; n];
    let mut runs = vec![
        Run {
            start: 0,
            finish: 0,
            worker: 0,
        };
        n
    ];
    let mut used = vec![0u64; capacity.len()];
    let mut running = Vec::<Running>::new();
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
    let mut done_count = 0usize;
    let mut makespan = 0u64;

    while done_count < n {
        // Complete every task finishing now in ascending task ID.
        let mut finishing = running
            .iter()
            .filter(|r| r.finish == now)
            .map(|r| r.task)
            .collect::<Vec<_>>();
        finishing.sort_unstable();

        for task_id in finishing {
            let pos = running.iter().position(|r| r.task == task_id).unwrap();
            let run = running.remove(pos);
            completed[task_id] = true;
            done_count += 1;
            makespan = makespan.max(run.finish);
            for (r, &need) in tasks[task_id].needs.iter().enumerate() {
                used[r] -= u64::from(need);
            }
            write!(
                &mut trace,
                "{}|end|{}|{}|",
                now, task_id, run.worker
            )
            .unwrap();
            write_usage(&mut trace, &used);
            trace.push('\n');
        }

        let mut started_any = false;
        loop {
            let mut candidates = (0..n)
                .filter(|&id| {
                    !started[id]
                        && tasks[id].release <= now
                        && tasks[id].deps.iter().all(|&dep| completed[dep])
                        && tasks[id]
                            .needs
                            .iter()
                            .enumerate()
                            .all(|(r, &need)| {
                                u64::from(need) <= u64::from(capacity[r]) - used[r]
                            })
                })
                .collect::<Vec<_>>();

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
                .find(|&w| worker_free[w] <= now);

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
            worker_free[worker] = available_at;
            for (r, &need) in tasks[task_id].needs.iter().enumerate() {
                used[r] += u64::from(need);
            }
            runs[task_id] = Run {
                start: now,
                finish,
                worker,
            };
            running.push(Running {
                task: task_id,
                worker,
                finish,
            });
            started_any = true;

            write!(
                &mut trace,
                "{}|start|{}|{}|",
                now, task_id, worker
            )
            .unwrap();
            write_usage(&mut trace, &used);
            trace.push('\n');

            if finish == now {
                let pos = running.iter().position(|r| r.task == task_id).unwrap();
                running.remove(pos);
                completed[task_id] = true;
                done_count += 1;
                makespan = makespan.max(finish);
                for (r, &need) in tasks[task_id].needs.iter().enumerate() {
                    used[r] -= u64::from(need);
                }
                write!(
                    &mut trace,
                    "{}|end|{}|{}|",
                    now, task_id, worker
                )
                .unwrap();
                write_usage(&mut trace, &used);
                trace.push('\n');
            }
        }

        if done_count == n {
            break;
        }

        let mut next_time: Option<u64> = None;
        let mut consider = |time: u64| {
            if time > now {
                next_time = Some(next_time.map_or(time, |current| current.min(time)));
            }
        };

        for run in &running {
            consider(run.finish);
        }
        for id in 0..n {
            if !started[id] {
                consider(tasks[id].release);
            }
        }

        // Cooldown expiry matters only when it can make a worker available.
        if !started_any {
            for &time in &worker_free {
                consider(time);
            }
        }

        let Some(next) = next_time else {
            // A nonempty unfinished DAG always has a future release, finish,
            // or worker expiry. This guard avoids any panic if invariants change.
            return Err(Error::Cycle {
                blocked: (0..n).filter(|&id| !completed[id]).collect(),
            });
        };
        now = next;
    }

    writeln!(&mut trace, "makespan={}", makespan).unwrap();

    Ok(Schedule {
        runs,
        makespan,
        trace,
    })
}

fn write_usage(out: &mut String, used: &[u64]) {
    if used.is_empty() {
        out.push('-');
    } else {
        for (index, amount) in used.iter().enumerate() {
            if index != 0 {
                out.push(',');
            }
            use std::fmt::Write as _;
            write!(out, "{}", amount).unwrap();
        }
    }
}