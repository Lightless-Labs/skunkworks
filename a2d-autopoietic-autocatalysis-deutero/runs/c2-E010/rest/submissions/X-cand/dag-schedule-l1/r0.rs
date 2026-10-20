use std::cmp::Reverse;
use std::collections::BinaryHeap;

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
    BadDependency { task: usize, position: usize, value: usize },
    SelfDependency { task: usize, position: usize },
    DuplicateDependency { task: usize, position: usize, value: usize },
    Cycle { blocked: Vec<usize> },
    Overflow { task: usize },
}

pub fn earliest(tasks: &[Task]) -> Result<Plan, Error> {
    let n = tasks.len();

    // Validate all dependency entries before building or traversing the graph.
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

    // Each dependency is an incoming edge; build predecessor-to-child adjacency.
    let mut children = vec![Vec::new(); n];
    let mut indegree = vec![0usize; n];
    for (child, task) in tasks.iter().enumerate() {
        indegree[child] = task.deps.len();
        for &parent in &task.deps {
            children[parent].push(child);
        }
    }

    let mut available = BinaryHeap::new();
    for id in 0..n {
        if indegree[id] == 0 {
            available.push(Reverse(id));
        }
    }

    let mut order = Vec::with_capacity(n);
    let mut remaining = indegree.clone();
    while let Some(Reverse(id)) = available.pop() {
        order.push(id);
        for &child in &children[id] {
            remaining[child] -= 1;
            if remaining[child] == 0 {
                available.push(Reverse(child));
            }
        }
    }

    if order.len() != n {
        let mut removed = vec![false; n];
        for &id in &order {
            removed[id] = true;
        }
        let blocked = (0..n).filter(|&id| !removed[id]).collect();
        return Err(Error::Cycle { blocked });
    }

    let mut times = vec![Timing { start: 0, finish: 0 }; n];
    let mut makespan = 0u64;

    for &id in &order {
        let start = tasks[id]
            .deps
            .iter()
            .map(|&parent| times[parent].finish)
            .max()
            .unwrap_or(0);

        let finish = match start.checked_add(tasks[id].duration) {
            Some(value) => value,
            None => return Err(Error::Overflow { task: id }),
        };

        times[id] = Timing { start, finish };
        makespan = makespan.max(finish);
    }

    Ok(Plan {
        order,
        times,
        makespan,
    })
}
