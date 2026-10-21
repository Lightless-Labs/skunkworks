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
    let task_count = tasks.len();

    // Validate all dependency entries before constructing the graph.
    for (task_id, task) in tasks.iter().enumerate() {
        for (position, &value) in task.deps.iter().enumerate() {
            if value >= task_count {
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

    // Build edges from each predecessor to the task it unblocks.
    let mut children = vec![Vec::new(); task_count];
    let mut indegree = vec![0usize; task_count];
    for (task_id, task) in tasks.iter().enumerate() {
        indegree[task_id] = task.deps.len();
        for &predecessor in &task.deps {
            children[predecessor].push(task_id);
        }
    }

    // Kahn traversal with the smallest available task ID chosen each time.
    let mut available = std::collections::BTreeSet::new();
    for (task_id, &degree) in indegree.iter().enumerate() {
        if degree == 0 {
            available.insert(task_id);
        }
    }

    let mut order = Vec::with_capacity(task_count);
    let mut remaining_indegree = indegree;
    while let Some(task_id) = available.pop_first() {
        order.push(task_id);
        for &child in &children[task_id] {
            remaining_indegree[child] -= 1;
            if remaining_indegree[child] == 0 {
                available.insert(child);
            }
        }
    }

    if order.len() != task_count {
        let mut blocked = Vec::with_capacity(task_count - order.len());
        let mut removed = vec![false; task_count];
        for task_id in order {
            removed[task_id] = true;
        }
        for (task_id, was_removed) in removed.into_iter().enumerate() {
            if !was_removed {
                blocked.push(task_id);
            }
        }
        return Err(Error::Cycle { blocked });
    }

    let mut times = vec![Timing { start: 0, finish: 0 }; task_count];
    let mut makespan = 0;

    for &task_id in &order {
        let start = tasks[task_id]
            .deps
            .iter()
            .map(|&predecessor| times[predecessor].finish)
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