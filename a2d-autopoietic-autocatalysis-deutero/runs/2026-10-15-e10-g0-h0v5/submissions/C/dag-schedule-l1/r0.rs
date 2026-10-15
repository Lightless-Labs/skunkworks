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
    let count = tasks.len();

    // Validate every dependency before cycle detection or arithmetic.
    for (task_id, task) in tasks.iter().enumerate() {
        let mut seen = std::collections::HashSet::new();
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
            if !seen.insert(value) {
                return Err(Error::DuplicateDependency {
                    task: task_id,
                    position,
                    value,
                });
            }
        }
    }

    // Build indegrees and predecessor-to-child adjacency lists.
    let mut indegree = vec![0usize; count];
    let mut children = vec![Vec::new(); count];
    for (task_id, task) in tasks.iter().enumerate() {
        indegree[task_id] = task.deps.len();
        for &predecessor in &task.deps {
            children[predecessor].push(task_id);
        }
    }

    // Kahn traversal with the smallest currently available ID first.
    let mut available = std::collections::BinaryHeap::new();
    for id in 0..count {
        if indegree[id] == 0 {
            available.push(std::cmp::Reverse(id));
        }
    }

    let mut order = Vec::with_capacity(count);
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

    if order.len() != count {
        let removed = {
            let mut set = vec![false; count];
            for &id in &order {
                set[id] = true;
            }
            set
        };
        let blocked = (0..count).filter(|&id| !removed[id]).collect();
        return Err(Error::Cycle { blocked });
    }

    // Compute earliest timings in Kahn order.
    let mut times = vec![Timing { start: 0, finish: 0 }; count];
    let mut makespan = 0u64;
    for &id in &order {
        let start = tasks[id]
            .deps
            .iter()
            .map(|&predecessor| times[predecessor].finish)
            .max()
            .unwrap_or(0);
        let finish = start
            .checked_add(tasks[id].duration)
            .ok_or(Error::Overflow { task: id })?;
        times[id] = Timing { start, finish };
        makespan = makespan.max(finish);
    }

    Ok(Plan {
        order,
        times,
        makespan,
    })
}

fn main() {}