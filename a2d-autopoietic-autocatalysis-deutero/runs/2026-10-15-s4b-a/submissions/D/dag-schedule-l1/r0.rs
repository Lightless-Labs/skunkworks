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

    // Validate every dependency before doing any scheduling work.
    for (task_id, task) in tasks.iter().enumerate() {
        for (position, &dependency) in task.deps.iter().enumerate() {
            if dependency >= n {
                return Err(Error::BadDependency {
                    task: task_id,
                    position,
                    value: dependency,
                });
            }
            if dependency == task_id {
                return Err(Error::SelfDependency {
                    task: task_id,
                    position,
                });
            }
            if task.deps[..position].contains(&dependency) {
                return Err(Error::DuplicateDependency {
                    task: task_id,
                    position,
                    value: dependency,
                });
            }
        }
    }

    // Build predecessor-to-child adjacency lists and indegrees.
    let mut children = vec![Vec::new(); n];
    let mut indegree = vec![0usize; n];
    for (task_id, task) in tasks.iter().enumerate() {
        indegree[task_id] = task.deps.len();
        for &dependency in &task.deps {
            children[dependency].push(task_id);
        }
    }

    // A sorted vector provides the required smallest-currently-available order.
    let mut available = Vec::new();
    for (task_id, &degree) in indegree.iter().enumerate() {
        if degree == 0 {
            available.push(task_id);
        }
    }

    let mut order = Vec::with_capacity(n);
    let mut cursor = 0;
    while cursor < available.len() {
        let task_id = available.remove(0);
        order.push(task_id);

        for &child in &children[task_id] {
            indegree[child] -= 1;
            if indegree[child] == 0 {
                let insertion = available.partition_point(|&id| id < child);
                available.insert(insertion, child);
            }
        }
        cursor = 0;
    }

    if order.len() != n {
        let mut removed = vec![false; n];
        for &task_id in &order {
            removed[task_id] = true;
        }
        let blocked = (0..n).filter(|&id| !removed[id]).collect();
        return Err(Error::Cycle { blocked });
    }

    // Compute timings in Kahn order.
    let mut starts = vec![0u64; n];
    let mut times = vec![Timing { start: 0, finish: 0 }; n];
    let mut makespan = 0;

    for &task_id in &order {
        let start = starts[task_id];
        let finish = start
            .checked_add(tasks[task_id].duration)
            .ok_or(Error::Overflow { task: task_id })?;

        times[task_id] = Timing { start, finish };
        makespan = makespan.max(finish);

        for &child in &children[task_id] {
            starts[child] = starts[child].max(finish);
        }
    }

    Ok(Plan {
        order,
        times,
        makespan,
    })
}