#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Task {
    pub duration: u64,
    pub deps: Vec<usize>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Timing {
    pub start: u64,
    pub finish: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Plan {
    pub order: Vec<usize>,
    pub times: Vec<Timing>,
    pub makespan: u64,
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
    Overflow {
        task: usize,
    },
}

pub fn earliest(tasks: &[Task]) -> Result<Plan, Error> {
    let n = tasks.len();

    // Validate all dependency entries before cycle detection or timing.
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

    // Build predecessor counts and child lists.
    let mut indegree = vec![0usize; n];
    let mut children = vec![Vec::new(); n];
    for (task_id, task) in tasks.iter().enumerate() {
        indegree[task_id] = task.deps.len();
        for &dependency in &task.deps {
            children[dependency].push(task_id);
        }
    }

    // Kahn traversal, choosing the smallest currently available ID each time.
    let mut available = Vec::new();
    for (task_id, &count) in indegree.iter().enumerate() {
        if count == 0 {
            available.push(task_id);
        }
    }

    let mut order = Vec::with_capacity(n);
    let mut removed = vec![false; n];
    while !available.is_empty() {
        let smallest_pos = available
            .iter()
            .enumerate()
            .min_by_key(|&(_, task_id)| *task_id)
            .map(|(position, _)| position)
            .unwrap();
        let task_id = available.swap_remove(smallest_pos);
        removed[task_id] = true;
        order.push(task_id);

        for &child in &children[task_id] {
            indegree[child] -= 1;
            if indegree[child] == 0 {
                available.push(child);
            }
        }
    }

    if order.len() != n {
        let blocked = removed
            .iter()
            .enumerate()
            .filter_map(|(task_id, &was_removed)| (!was_removed).then_some(task_id))
            .collect();
        return Err(Error::Cycle { blocked });
    }

    let mut times = vec![Timing { start: 0, finish: 0 }; n];
    let mut makespan = 0;

    for &task_id in &order {
        let start = tasks[task_id]
            .deps
            .iter()
            .map(|&dependency| times[dependency].finish)
            .max()
            .unwrap_or(0);
        let finish = start
            .checked_add(tasks[task_id].duration)
            .ok_or(Error::Overflow { task: task_id })?;

        times[task_id] = Timing { start, finish };
        makespan = makespan.max(finish);
    }

    Ok(Plan {
        order,
        times,
        makespan,
    })
}

fn main() {}