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
    use std::collections::BTreeSet;

    let n = tasks.len();

    // Validate every dependency before cycle detection or timing computation.
    for (task_id, task) in tasks.iter().enumerate() {
        let mut seen = BTreeSet::new();
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
            if !seen.insert(value) {
                return Err(Error::DuplicateDependency {
                    task: task_id,
                    position,
                    value,
                });
            }
        }
    }

    // Build child lists and indegrees from the validated predecessor lists.
    let mut children = vec![Vec::new(); n];
    let mut indegree = vec![0usize; n];
    for (task_id, task) in tasks.iter().enumerate() {
        indegree[task_id] = task.deps.len();
        for &predecessor in &task.deps {
            children[predecessor].push(task_id);
        }
    }

    // Kahn traversal, always selecting the smallest currently available ID.
    let mut available = BTreeSet::new();
    for (task_id, &degree) in indegree.iter().enumerate() {
        if degree == 0 {
            available.insert(task_id);
        }
    }

    let mut order = Vec::with_capacity(n);
    while let Some(&task_id) = available.iter().next() {
        available.remove(&task_id);
        order.push(task_id);

        for &child in &children[task_id] {
            indegree[child] -= 1;
            if indegree[child] == 0 {
                available.insert(child);
            }
        }
    }

    if order.len() != n {
        let blocked = (0..n)
            .filter(|&task_id| indegree[task_id] != 0)
            .collect();
        return Err(Error::Cycle { blocked });
    }

    // Compute timings only after full cycle detection.
    let mut starts = vec![0u64; n];
    let mut finishes = vec![0u64; n];

    for &task_id in &order {
        let start = tasks[task_id]
            .deps
            .iter()
            .map(|&predecessor| finishes[predecessor])
            .max()
            .unwrap_or(0);

        let finish = start
            .checked_add(tasks[task_id].duration)
            .ok_or(Error::Overflow { task: task_id })?;

        starts[task_id] = start;
        finishes[task_id] = finish;
    }

    let makespan = finishes.iter().copied().max().unwrap_or(0);
    let times = (0..n)
        .map(|task_id| Timing {
            start: starts[task_id],
            finish: finishes[task_id],
        })
        .collect();

    Ok(Plan {
        order,
        times,
        makespan,
    })
}

fn main() {}