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

    // Validate every dependency before doing any graph computation.
    for (task_id, task) in tasks.iter().enumerate() {
        for (position, &dependency) in task.deps.iter().enumerate() {
            if dependency >= task_count {
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

    // Build successor lists and indegrees.
    let mut children = vec![Vec::new(); task_count];
    let mut indegree = vec![0usize; task_count];

    for (task_id, task) in tasks.iter().enumerate() {
        indegree[task_id] = task.deps.len();
        for &dependency in &task.deps {
            children[dependency].push(task_id);
        }
    }

    // A sorted vector acts as the available set; insertions preserve the
    // smallest-ID-first Kahn traversal rule.
    let mut available = Vec::new();
    for task_id in 0..task_count {
        if indegree[task_id] == 0 {
            available.push(task_id);
        }
    }

    let mut order = Vec::with_capacity(task_count);
    while !available.is_empty() {
        let task_id = available.remove(0);
        order.push(task_id);

        for &child in &children[task_id] {
            indegree[child] -= 1;
            if indegree[child] == 0 {
                let insertion = available.partition_point(|&id| id < child);
                available.insert(insertion, child);
            }
        }
    }

    if order.len() != task_count {
        let mut blocked = Vec::new();
        for (task_id, &remaining_indegree) in indegree.iter().enumerate() {
            if remaining_indegree > 0 {
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
            .map(|&dependency| times[dependency].finish)
            .max()
            .unwrap_or(0);

        let finish = match start.checked_add(tasks[task_id].duration) {
            Some(finish) => finish,
            None => return Err(Error::Overflow { task: task_id }),
        };

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