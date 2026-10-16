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
    let count = tasks.len();

    // Validate every dependency before doing any graph computation.
    for (task_id, task) in tasks.iter().enumerate() {
        let mut seen = Vec::new();
        for (position, &value) in task.deps.iter().enumerate() {
            if value >= count {
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

    // Build successor lists and indegrees. Dependencies point from predecessor
    // to task for the purposes of Kahn traversal.
    let mut children = vec![Vec::new(); count];
    let mut indegree = vec![0usize; count];
    for (task_id, task) in tasks.iter().enumerate() {
        indegree[task_id] = task.deps.len();
        for &predecessor in &task.deps {
            children[predecessor].push(task_id);
        }
    }

    // Keep available IDs sorted so each step removes the smallest currently
    // available task, including newly unblocked tasks.
    let mut available = Vec::new();
    for (id, &degree) in indegree.iter().enumerate() {
        if degree == 0 {
            available.push(id);
        }
    }

    let mut order = Vec::with_capacity(count);
    let mut cursor = 0usize;
    while cursor < available.len() {
        let id = available.remove(cursor);
        order.push(id);

        for &child in &children[id] {
            indegree[child] -= 1;
            if indegree[child] == 0 {
                let insert_at = available.partition_point(|&current| current < child);
                available.insert(insert_at, child);
            }
        }

        // The vector is kept sorted after each insertion, and removal is
        // always at index zero.
        cursor = 0;
    }

    if order.len() != count {
        let mut removed = vec![false; count];
        for &id in &order {
            removed[id] = true;
        }
        let blocked = (0..count).filter(|&id| !removed[id]).collect();
        return Err(Error::Cycle { blocked });
    }

    let mut starts = vec![0u64; count];
    let mut times = vec![Timing { start: 0, finish: 0 }; count];
    let mut makespan = 0u64;

    for &id in &order {
        let start = starts[id];
        let finish = start
            .checked_add(tasks[id].duration)
            .ok_or(Error::Overflow { task: id })?;

        times[id] = Timing { start, finish };
        makespan = makespan.max(finish);

        for &child in &children[id] {
            starts[child] = starts[child].max(finish);
        }
    }

    Ok(Plan {
        order,
        times,
        makespan,
    })
}