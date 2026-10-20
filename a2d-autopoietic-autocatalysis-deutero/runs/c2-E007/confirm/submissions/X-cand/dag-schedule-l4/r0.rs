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

use std::collections::BTreeSet;
use std::fmt::Write;

pub fn schedule(tasks: &[Task], cooldowns: &[u64], capacity: &[u32]) -> Result<Schedule, Error> {
    let n = tasks.len();

    // Validate every dependency entry before proceeding to any other validation.
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

    // Kahn traversal using a min-heap equivalent (BTreeSet).
    let mut indegree = vec![0usize; n];
    let mut children = vec![Vec::<usize>::new(); n];
    for (task_id, task) in tasks.iter().enumerate() {
        indegree[task_id] = task.deps.len();
        for &pred in &task.deps {
            children[pred].push(task_id);
        }
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
            let mut v = vec![false; n];
            for &id in &order {
                v[id] = true;
            }
            v
        };
        return Err(Error::Cycle {
            blocked: (0..n).filter(|&id| !removed[id]).collect(),
        });
    }

    let mut tails = vec![0u64; n];
    for &id in order.iter().rev() {
        let max_child = children[id].iter().map(|&child| tails[child]).max().unwrap_or(0);
        tails[id] = tasks[id].duration.checked_add(max_child).ok_or(Error::Overflow {
            task: id,
            stage: Stage::Tail,
        })?;
    }

    let workers = cooldowns.len();
    let resources = capacity.len();
    let mut used = vec![0u64; resources];
    let mut started = vec![false; n];
    let mut completed = vec![false; n];
    let mut runs: Vec<Option<Run>> = vec![None; n];
    let mut worker_available = vec![0u64; workers];
    let mut running: Vec<(u64, usize, usize)> = Vec::new(); // finish, task, worker
    let mut trace = String::new();
    writeln!(&mut trace, "workers={};resources={}", workers, resources).unwrap();
    let mut now = 0u64;
    let mut completed_count = 0usize;
    let mut makespan = 0u64;

    while completed_count < n {
        // Complete all tasks finishing now in task-ID order.
        running.sort_unstable_by_key(|&(finish, task, _)| (finish, task));
        let mut remaining = Vec::with_capacity(running.len());
        for (finish, id, worker) in running.drain(..) {
            if finish == now {
                completed[id] = true;
                completed_count += 1;
                makespan = makespan.max(finish);
                for r in 0..resources {
                    used[r] -= u64::from(tasks[id].needs[r]);
                }
                write!(&mut trace, "{}|end|{}|{}|", now, id, worker).unwrap();
                write_usage(&mut trace, &used);
                trace.push('\n');
            } else {
                remaining.push((finish, id, worker));
            }
        }
        running = remaining;

        loop {
            let mut candidate: Option<usize> = None;
            for id in 0..n {
                if started[id] || tasks[id].release > now {
                    continue;
                }
                if !tasks[id].deps.iter().all(|&pred| completed[pred]) {
                    continue;
                }
                if (0..resources).any(|r| {
                    u64::from(tasks[id].needs[r]) > u64::from(capacity[r]) - used[r]
                }) {
                    continue;
                }
                let better = match candidate {
                    None => true,
                    Some(current) => {
                        tails[id] > tails[current]
                            || (tails[id] == tails[current]
                                && (tasks[id].priority > tasks[current].priority
                                    || (tasks[id].priority == tasks[current].priority && id < current)))
                    }
                };
                if better {
                    candidate = Some(id);
                }
            }

            let Some(id) = candidate else { break };

            let worker = (0..workers)
                .map(|offset| (id % workers + offset) % workers)
                .find(|&w| worker_available[w] <= now);

            let Some(worker) = worker else { break };

            let finish = now.checked_add(tasks[id].duration).ok_or(Error::Overflow {
                task: id,
                stage: Stage::Finish,
            })?;
            let turnaround = finish.checked_add(cooldowns[worker]).ok_or(Error::Overflow {
                task: id,
                stage: Stage::Turnaround,
            })?;

            started[id] = true;
            worker_available[worker] = turnaround;
            for r in 0..resources {
                used[r] += u64::from(tasks[id].needs[r]);
            }
            runs[id] = Some(Run { start: now, finish, worker });
            running.push((finish, id, worker));
            write!(&mut trace, "{}|start|{}|{}|", now, id, worker).unwrap();
            write_usage(&mut trace, &used);
            trace.push('\n');

            // A newly started zero-duration task completes immediately.
            if finish == now {
                running.retain(|&(_, task, _)| task != id);
                completed[id] = true;
                completed_count += 1;
                makespan = makespan.max(finish);
                for r in 0..resources {
                    used[r] -= u64::from(tasks[id].needs[r]);
                }
                write!(&mut trace, "{}|end|{}|{}|", now, id, worker).unwrap();
                write_usage(&mut trace, &used);
                trace.push('\n');
            }
        }

        if completed_count == n {
            break;
        }

        let mut next_time: Option<u64> = None;
        for &(finish, _, _) in &running {
            if finish > now {
                next_time = Some(next_time.map_or(finish, |t| t.min(finish)));
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

        // If no event can advance time, all remaining tasks are necessarily waiting
        // on a finite release, running completion, or cooldown; this is defensive.
        now = next_time.expect("valid acyclic schedule must have a next event");
    }

    writeln!(&mut trace, "makespan={}", makespan).unwrap();
    Ok(Schedule {
        runs: runs.into_iter().map(Option::unwrap).collect(),
        makespan,
        trace,
    })
}

fn write_usage(trace: &mut String, used: &[u64]) {
    if used.is_empty() {
        trace.push('-');
    } else {
        for (index, value) in used.iter().enumerate() {
            if index != 0 {
                trace.push(',');
            }
            write!(trace, "{}", value).unwrap();
        }
    }
}