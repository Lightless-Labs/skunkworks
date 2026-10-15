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

pub fn schedule(tasks: &[Task], cooldowns: &[u64], capacity: &[u32]) -> Result<Schedule, Error> {
    use std::collections::BTreeSet;

    let n = tasks.len();

    // Validate every dependency entry before any other checks.
    for (task_id, task) in tasks.iter().enumerate() {
        let mut seen = BTreeSet::new();
        for (position, &value) in task.deps.iter().enumerate() {
            if value >= n {
                return Err(Error::BadDependency { task: task_id, position, value });
            }
            if value == task_id {
                return Err(Error::SelfDependency { task: task_id, position });
            }
            if !seen.insert(value) {
                return Err(Error::DuplicateDependency { task: task_id, position, value });
            }
        }
    }

    if cooldowns.is_empty() || cooldowns.len() > 64 {
        return Err(Error::Workers { count: cooldowns.len() });
    }

    for (task_id, task) in tasks.iter().enumerate() {
        if task.needs.len() != capacity.len() {
            return Err(Error::ResourceShape {
                task: task_id,
                expected: capacity.len(),
                actual: task.needs.len(),
            });
        }
        for (resource, (&need, &available)) in task.needs.iter().zip(capacity).enumerate() {
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

    // Build successor lists and perform deterministic Kahn traversal.
    let mut indegree = vec![0usize; n];
    let mut children = vec![Vec::<usize>::new(); n];
    for (task_id, task) in tasks.iter().enumerate() {
        indegree[task_id] = task.deps.len();
        for &predecessor in &task.deps {
            children[predecessor].push(task_id);
        }
    }
    for list in &mut children {
        list.sort_unstable();
    }

    let mut available = BTreeSet::new();
    for id in 0..n {
        if indegree[id] == 0 {
            available.insert(id);
        }
    }

    let mut order = Vec::with_capacity(n);
    while let Some(id) = available.pop_first() {
        order.push(id);
        for &child in &children[id] {
            indegree[child] -= 1;
            if indegree[child] == 0 {
                available.insert(child);
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
        let blocked = (0..n).filter(|&id| !removed[id]).collect();
        return Err(Error::Cycle { blocked });
    }

    // Compute tails in reverse Kahn order.
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
            .ok_or(Error::Overflow { task: id, stage: Stage::Tail })?;
    }

    #[derive(Clone, Copy)]
    struct Running {
        task: usize,
        finish: u64,
        worker: usize,
    }

    let worker_count = cooldowns.len();
    let mut worker_available = vec![0u64; worker_count];
    let mut started = vec![false; n];
    let mut completed = vec![false; n];
    let mut runs: Vec<Option<Run>> = vec![None; n];
    let mut running = Vec::<Running>::new();
    let mut used = vec![0u64; capacity.len()];
    let mut trace = format!("workers={};resources={}\n", worker_count, capacity.len());
    let mut now = 0u64;
    let mut completed_count = 0usize;
    let mut makespan = 0u64;

    fn append_usage(trace: &mut String, used: &[u64]) {
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
        trace.push('\n');
    }

    while completed_count < n {
        // Complete all tasks finishing now, in task ID order.
        let mut ending: Vec<Running> = running.iter().copied().filter(|r| r.finish == now).collect();
        ending.sort_by_key(|r| r.task);
        if !ending.is_empty() {
            for end in &ending {
                for (r, &need) in tasks[end.task].needs.iter().enumerate() {
                    used[r] -= u64::from(need);
                }
                completed[end.task] = true;
                completed_count += 1;
                makespan = makespan.max(end.finish);
                trace.push_str(&format!("{}|end|{}|{}|", now, end.task, end.worker));
                append_usage(&mut trace, &used);
            }
            running.retain(|r| r.finish != now);
        }

        // Start one eligible task at a time, re-evaluating after each admission.
        loop {
            let mut candidates: Vec<usize> = (0..n)
                .filter(|&id| {
                    !started[id]
                        && tasks[id].release <= now
                        && tasks[id].deps.iter().all(|&pred| completed[pred])
                        && tasks[id]
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
                .find(|&w| worker_available[w] <= now)
                .expect("a selected admission has an available worker");

            let finish = now.checked_add(tasks[task_id].duration).ok_or(Error::Overflow {
                task: task_id,
                stage: Stage::Finish,
            })?;
            let worker_ready = finish.checked_add(cooldowns[worker]).ok_or(Error::Overflow {
                task: task_id,
                stage: Stage::Turnaround,
            })?;

            started[task_id] = true;
            worker_available[worker] = worker_ready;
            for (r, &need) in tasks[task_id].needs.iter().enumerate() {
                used[r] += u64::from(need);
            }
            runs[task_id] = Some(Run { start: now, finish, worker });
            trace.push_str(&format!("{}|start|{}|{}|", now, task_id, worker));
            append_usage(&mut trace, &used);

            if finish == now {
                // Immediate completion of a zero-duration task.
                for (r, &need) in tasks[task_id].needs.iter().enumerate() {
                    used[r] -= u64::from(need);
                }
                completed[task_id] = true;
                completed_count += 1;
                makespan = makespan.max(finish);
                trace.push_str(&format!("{}|end|{}|{}|", now, task_id, worker));
                append_usage(&mut trace, &used);
            } else {
                running.push(Running { task: task_id, finish, worker });
            }
        }

        if completed_count == n {
            break;
        }

        // Advance to the earliest future event that can change eligibility.
        let mut next = None::<u64>;
        for r in &running {
            if r.finish > now {
                next = Some(next.map_or(r.finish, |v| v.min(r.finish)));
            }
        }
        for id in 0..n {
            if !started[id] && tasks[id].release > now {
                next = Some(next.map_or(tasks[id].release, |v| v.min(tasks[id].release)));
            }
        }
        for w in 0..worker_count {
            if worker_available[w] > now {
                next = Some(next.map_or(worker_available[w], |v| v.min(worker_available[w])));
            }
        }

        now = next.expect("unfinished tasks always have a future event");
    }

    for run in &runs {
        debug_assert!(run.is_some());
    }
    trace.push_str(&format!("makespan={}\n", makespan));

    Ok(Schedule {
        runs: runs.into_iter().map(Option::unwrap).collect(),
        makespan,
        trace,
    })
}

fn main() {}