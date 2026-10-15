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

    // Validate every dependency before doing any graph traversal or arithmetic.
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

    // Edges point from each predecessor to its dependent task.
    let mut children = vec![Vec::new(); n];
    let mut indegree = vec![0usize; n];
    for (task_id, task) in tasks.iter().enumerate() {
        indegree[task_id] = task.deps.len();
        for &dep in &task.deps {
            children[dep].push(task_id);
        }
    }

    // BinaryHeap is a max heap, so store reversed IDs to remove the smallest first.
    let mut available = BinaryHeap::new();
    for id in 0..n {
        if indegree[id] == 0 {
            available.push(std::cmp::Reverse(id));
        }
    }

    let mut order = Vec::with_capacity(n);
    let mut remaining_indegree = indegree;
    while let Some(std::cmp::Reverse(id)) = available.pop() {
        order.push(id);
        for &child in &children[id] {
            remaining_indegree[child] -= 1;
            if remaining_indegree[child] == 0 {
                available.push(std::cmp::Reverse(child));
            }
        }
    }

    if order.len() != n {
        let removed = {
            let mut flags = vec![false; n];
            for &id in &order {
                flags[id] = true;
            }
            flags
        };
        let blocked = (0..n).filter(|&id| !removed[id]).collect();
        return Err(Error::Cycle { blocked });
    }

    let mut starts = vec![0u64; n];
    let mut times: Vec<Option<Timing>> = vec![None; n];

    for &id in &order {
        let start = starts[id];
        let finish = start
            .checked_add(tasks[id].duration)
            .ok_or(Error::Overflow { task: id })?;

        times[id] = Some(Timing { start, finish });

        for &child in &children[id] {
            if finish > starts[child] {
                starts[child] = finish;
            }
        }
    }

    let mut makespan = 0u64;
    let times: Vec<Timing> = times
        .into_iter()
        .map(|timing| timing.expect("all tasks are visited in a complete Kahn traversal"))
        .collect();

    for timing in &times {
        makespan = makespan.max(timing.finish);
    }

    Ok(Plan {
        order,
        times,
        makespan,
    })
}

fn main() {}