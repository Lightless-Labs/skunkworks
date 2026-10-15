use std::collections::BinaryHeap;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Task {
    pub duration: u64,
    pub deps: Vec<usize>,
    pub release: u64,
    pub priority: i32,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Run {
    pub start: u64,
    pub finish: u64,
    pub worker: usize,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EventKind {
    Start,
    Finish,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Event {
    pub time: u64,
    pub task: usize,
    pub worker: usize,
    pub kind: EventKind,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Schedule {
    pub runs: Vec<Run>,
    pub makespan: u64,
    pub events: Vec<Event>,
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
}

pub fn schedule(tasks: &[Task], workers: usize, turnaround: u64) -> Result<Schedule, Error> {
    let n = tasks.len();

    // Validate every dependency before checking workers or doing graph work.
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

    if !(1..=64).contains(&workers) {
        return Err(Error::Workers { count: workers });
    }

    let mut indegree = vec![0usize; n];
    let mut children = vec![Vec::new(); n];
    for (task_id, task) in tasks.iter().enumerate() {
        indegree[task_id] = task.deps.len();
        for &dep in &task.deps {
            children[dep].push(task_id);
        }
    }

    // Min-heap behavior via Reverse.
    let mut ready = BinaryHeap::new();
    for id in 0..n {
        if indegree[id] == 0 {
            ready.push(std::cmp::Reverse(id));
        }
    }
    let mut topo = Vec::with_capacity(n);
    let mut remaining_indegree = indegree.clone();
    while let Some(std::cmp::Reverse(id)) = ready.pop() {
        topo.push(id);
        for &child in &children[id] {
            remaining_indegree[child] -= 1;
            if remaining_indegree[child] == 0 {
                ready.push(std::cmp::Reverse(child));
            }
        }
    }

    if topo.len() != n {
        let mut blocked = Vec::new();
        let mut removed = vec![false; n];
        for &id in &topo {
            removed[id] = true;
        }
        for id in 0..n {
            if !removed[id] {
                blocked.push(id);
            }
        }
        return Err(Error::Cycle { blocked });
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

    let mut runs: Vec<Option<Run>> = vec![None; n];
    let mut events = Vec::with_capacity(n.saturating_mul(2));
    let mut started = vec![false; n];
    let mut completed = vec![false; n];
    let mut remaining_deps: Vec<usize> = tasks.iter().map(|t| t.deps.len()).collect();

    let mut worker_available = vec![0u64; workers];
    let mut running: Vec<(u64, usize, usize)> = Vec::new(); // (finish, task, worker)
    let mut now = 0u64;
    let mut completed_count = 0usize;
    let mut makespan = 0u64;

    while completed_count < n {
        // Complete all currently due tasks in ascending task ID.
        let mut due: Vec<(u64, usize, usize)> = running
            .iter()
            .copied()
            .filter(|(finish, _, _)| *finish == now)
            .collect();
        due.sort_by_key(|&(_, task_id, _)| task_id);

        for (finish, task_id, worker_id) in &due {
            events.push(Event {
                time: *finish,
                task: *task_id,
                worker: *worker_id,
                kind: EventKind::Finish,
            });
            completed[*task_id] = true;
            completed_count += 1;
            makespan = makespan.max(*finish);
            for &child in &children[*task_id] {
                remaining_deps[child] -= 1;
            }
        }
        if !due.is_empty() {
            running.retain(|(finish, _, _)| *finish != now);
        }

        // Start one task at a time, re-evaluating eligibility and ranking each time.
        loop {
            let worker_id = worker_available
                .iter()
                .position(|&available_at| available_at <= now);
            let Some(worker_id) = worker_id else {
                break;
            };

            let mut eligible: Vec<usize> = (0..n)
                .filter(|&id| {
                    !started[id]
                        && tasks[id].release <= now
                        && remaining_deps[id] == 0
                })
                .collect();

            if eligible.is_empty() {
                break;
            }

            eligible.sort_by(|&a, &b| {
                tails[b]
                    .cmp(&tails[a])
                    .then_with(|| tasks[b].priority.cmp(&tasks[a].priority))
                    .then_with(|| a.cmp(&b))
            });
            let task_id = eligible[0];

            let finish = now
                .checked_add(tasks[task_id].duration)
                .ok_or(Error::Overflow {
                    task: task_id,
                    stage: Stage::Finish,
                })?;
            let available_at = finish.checked_add(turnaround).ok_or(Error::Overflow {
                task: task_id,
                stage: Stage::Turnaround,
            })?;

            started[task_id] = true;
            worker_available[worker_id] = available_at;
            runs[task_id] = Some(Run {
                start: now,
                finish,
                worker: worker_id,
            });
            events.push(Event {
                time: now,
                task: task_id,
                worker: worker_id,
                kind: EventKind::Start,
            });

            if finish == now {
                // A zero-duration task completes immediately before another admission.
                events.push(Event {
                    time: now,
                    task: task_id,
                    worker: worker_id,
                    kind: EventKind::Finish,
                });
                completed[task_id] = true;
                completed_count += 1;
                makespan = makespan.max(now);
                for &child in &children[task_id] {
                    remaining_deps[child] -= 1;
                }
            } else {
                running.push((finish, task_id, worker_id));
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

        for &(finish, _, _) in &running {
            consider(finish);
        }
        for id in 0..n {
            if !started[id] && tasks[id].release > now {
                consider(tasks[id].release);
            }
        }
        for &available_at in &worker_available {
            consider(available_at);
        }

        now = next_time.expect("unfinished tasks must have a future event");
    }

    let runs = runs
        .into_iter()
        .map(|run| run.expect("every task must start"))
        .collect();

    Ok(Schedule {
        runs,
        makespan,
        events,
    })
}

fn main() {}