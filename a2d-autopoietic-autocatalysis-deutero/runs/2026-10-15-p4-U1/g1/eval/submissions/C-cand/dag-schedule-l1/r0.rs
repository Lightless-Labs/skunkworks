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

    // Validate every dependency in the required task and list order.
    for (task_id, task) in tasks.iter().enumerate() {
        let mut seen = vec![false; count];
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

    // Build predecessor counts and successor lists without changing the input.
    let mut indegree: Vec<usize> = tasks.iter().map(|task| task.deps.len()).collect();
    let mut children = vec![Vec::new(); count];
    for (task_id, task) in tasks.iter().enumerate() {
        for &predecessor in &task.deps {
            children[predecessor].push(task_id);
        }
    }

    // Sorted available IDs, kept in ascending order.
    let mut available: Vec<usize> = (0..count).filter(|&id| indegree[id] == 0).collect();
    let mut order = Vec::with_capacity(count);

    while !available.is_empty() {
        let id = available.remove(0);
        order.push(id);

        for &child in &children[id] {
            indegree[child] -= 1;
            if indegree[child] == 0 {
                let insertion = available.binary_search(&child).unwrap_or_else(|index| index);
                available.insert(insertion, child);
            }
        }
    }

    if order.len() != count {
        let removed: Vec<bool> = {
            let mut flags = vec![false; count];
            for &id in &order {
                flags[id] = true;
            }
            flags
        };
        let blocked = (0..count).filter(|&id| !removed[id]).collect();
        return Err(Error::Cycle { blocked });
    }

    let mut starts = vec![0u64; count];
    let mut finishes = vec![0u64; count];

    for &id in &order {
        let start = tasks[id]
            .deps
            .iter()
            .map(|&predecessor| finishes[predecessor])
            .max()
            .unwrap_or(0);
        let finish = start
            .checked_add(tasks[id].duration)
            .ok_or(Error::Overflow { task: id })?;
        starts[id] = start;
        finishes[id] = finish;
    }

    let makespan = finishes.iter().copied().max().unwrap_or(0);
    let times = (0..count)
        .map(|id| Timing {
            start: starts[id],
            finish: finishes[id],
        })
        .collect();

    Ok(Plan {
        order,
        times,
        makespan,
    })
}

fn main() {}