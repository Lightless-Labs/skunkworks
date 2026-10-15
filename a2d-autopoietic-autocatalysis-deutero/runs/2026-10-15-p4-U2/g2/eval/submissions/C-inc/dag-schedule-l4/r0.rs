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

fn resource_string(used: &[u128]) -> String {
    if used.is_empty() {
        return "-".to_string();
    }
    let mut s = String::new();
    for (i, v) in used.iter().enumerate() {
        if i > 0 {
            s.push(',');
        }
        s.push_str(&v.to_string());
    }
    s
}

pub fn schedule(tasks: &[Task], cooldowns: &[u64], capacity: &[u32]) -> Result<Schedule, Error> {
    let n = tasks.len();

    // Validate every dependency before proceeding to any other phase.
    for (task_id, task) in tasks.iter().enumerate() {
        for (position, &value) in task.deps.iter().enumerate() {
            if value >= n {
                return Err(Error::BadDependency { task: task_id, position, value });
            }
            if value == task_id {
                return Err(Error::SelfDependency { task: task_id, position });
            }
            if task.deps[..position].contains(&value) {
                return Err(Error::DuplicateDependency { task: task_id, position, value });
            }
        }
    }

    let worker_count = cooldowns.len();
    if !(1..=64).contains(&worker_count) {
        return Err(Error::Workers { count: worker_count });
    }

    for (task_id, task) in tasks.iter().enumerate() {
        if task.needs.len() != capacity.len() {
            return Err(Error::ResourceShape {
                task: task_id,
                expected: capacity.len(),
                actual: task.needs.len(),
            });
        }
        for r in 0..capacity.len() {
            if task.needs[r] > capacity[r] {
                return Err(Error::ResourceExceeded {
                    task: task_id,
                    resource: r,
                    need: task.needs[r],
                    capacity: capacity[r],
                });
            }
        }
    }

    let mut children = vec![Vec::<usize>::new(); n];
    let mut indegree = vec![0usize; n];
    for (id, task) in tasks.iter().enumerate() {
        indegree[id] = task.deps.len();
        for &dep in &task.deps {
            children[dep].push(id);
        }
    }

    // Deterministic Kahn order: always remove the smallest currently available ID.
    let mut available = std::collections::BTreeSet::new();
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
        let removed = {
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
        let max_child = children[id].iter().map(|&c| tails[c]).max().unwrap_or(0);
        tails[id] = tasks[id]
            .duration
            .checked_add(max_child)
            .ok_or(Error::Overflow { task: id, stage: Stage::Tail })?;
    }

    let mut runs: Vec<Option<Run>> = vec![None; n];
    let mut started = vec![false; n];
    let mut completed = vec![false; n];
    let mut running: Vec<(usize, u64, usize)> = Vec::new(); // task, finish, worker
    let mut worker_free = vec![0u64; worker_count];
    let mut used = vec![0u128; capacity.len()];
    let mut now = 0u64;
    let mut done_count = 0usize;
    let mut makespan = 0u64;
    let mut trace = format!("workers={};resources={}\n", worker_count, capacity.len());

    while done_count < n {
        // Complete all tasks finishing now, in task-ID order.
        let mut ending: Vec<(usize, u64, usize)> =
            running.iter().copied().filter(|(_, finish, _)| *finish == now).collect();
        ending.sort_by_key(|(id, _, _)| *id);
        for (id, finish, worker) in &ending {
            for r in 0..capacity.len() {
                used[r] -= tasks[*id].needs[r] as u128;
            }
            completed[*id] = true;
            done_count += 1;
            makespan = makespan.max(*finish);
            trace.push_str(&format!(
                "{}|end|{}|{}|{}\n",
                now,
                id,
                worker,
                resource_string(&used)
            ));
        }
        if !ending.is_empty() {
            running.retain(|(_, finish, _)| *finish != now);
        }

        loop {
            let mut candidates = Vec::new();
            for id in 0..n {
                if started[id] || tasks[id].release > now {
                    continue;
                }
                if tasks[id].deps.iter().any(|&dep| !completed[dep]) {
                    continue;
                }
                let fits = (0..capacity.len())
                    .all(|r| tasks[id].needs[r] as u128 <= capacity[r] as u128 - used[r]);
                if fits {
                    candidates.push(id);
                }
            }
            candidates.sort_by(|&a, &b| {
                tails[b]
                    .cmp(&tails[a])
                    .then_with(|| tasks[b].priority.cmp(&tasks[a].priority))
                    .then_with(|| a.cmp(&b))
            });

            let mut selected = None;
            let mut selected_worker = 0usize;
            for id in candidates {
                let preferred = id % worker_count;
                for offset in 0..worker_count {
                    let w = (preferred + offset) % worker_count;
                    if worker_free[w] <= now {
                        selected = Some(id);
                        selected_worker = w;
                        break;
                    }
                }
                if selected.is_some() {
                    break;
                }
            }
            let Some(id) = selected else {
                break;
            };

            let finish = now.checked_add(tasks[id].duration).ok_or(Error::Overflow {
                task: id,
                stage: Stage::Finish,
            })?;
            let available_at = finish.checked_add(cooldowns[selected_worker]).ok_or(
                Error::Overflow { task: id, stage: Stage::Turnaround },
            )?;

            started[id] = true;
            worker_free[selected_worker] = available_at;
            runs[id] = Some(Run { start: now, finish, worker: selected_worker });
            for r in 0..capacity.len() {
                used[r] += tasks[id].needs[r] as u128;
            }
            running.push((id, finish, selected_worker));
            trace.push_str(&format!(
                "{}|start|{}|{}|{}\n",
                now,
                id,
                selected_worker,
                resource_string(&used)
            ));

            if finish == now {
                // A zero-duration task completes immediately before another admission.
                for r in 0..capacity.len() {
                    used[r] -= tasks[id].needs[r] as u128;
                }
                completed[id] = true;
                done_count += 1;
                makespan = makespan.max(finish);
                trace.push_str(&format!(
                    "{}|end|{}|{}|{}\n",
                    now,
                    id,
                    selected_worker,
                    resource_string(&used)
                ));
                running.retain(|(running_id, _, _)| *running_id != id);
            }
        }

        if done_count == n {
            break;
        }

        let mut next_time: Option<u64> = None;
        for &(_, finish, _) in &running {
            if finish > now {
                next_time = Some(next_time.map_or(finish, |t| t.min(finish)));
            }
        }
        for id in 0..n {
            if !started[id] && tasks[id].release > now {
                next_time = Some(next_time.map_or(tasks[id].release, |t| t.min(tasks[id].release)));
            }
        }
        for &free_at in &worker_free {
            if free_at > now {
                next_time = Some(next_time.map_or(free_at, |t| t.min(free_at)));
            }
        }
        let Some(next) = next_time else {
            // With an acyclic graph and validated resources, lack of a future event
            // would indicate no remaining task can make progress.
            return Err(Error::Cycle {
                blocked: (0..n).filter(|&id| !completed[id]).collect(),
            });
        };
        now = next;
    }

    let runs = runs.into_iter().map(Option::unwrap).collect();
    trace.push_str(&format!("makespan={}\n", makespan));
    Ok(Schedule { runs, makespan, trace })
}

fn main() {}