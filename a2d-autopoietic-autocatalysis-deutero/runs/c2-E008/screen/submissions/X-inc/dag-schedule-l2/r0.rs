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
pub struct Analysis {
    pub order: Vec<usize>,
    pub earliest: Vec<Timing>,
    pub latest: Vec<Timing>,
    pub slack: Vec<u64>,
    pub critical: Vec<usize>,
    pub path: Vec<usize>,
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
    TargetTooSmall {
        minimum: u64,
    },
}

pub fn analyze(tasks: &[Task], target: u64) -> Result<Analysis, Error> {
    let count = tasks.len();

    // Validate every dependency before constructing or traversing the graph.
    for (task_id, task) in tasks.iter().enumerate() {
        let mut seen = Vec::with_capacity(task.deps.len());
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

    // Build successor lists and indegrees from the validated dependencies.
    let mut successors = vec![Vec::new(); count];
    let mut indegree = vec![0usize; count];
    for (task_id, task) in tasks.iter().enumerate() {
        indegree[task_id] = task.deps.len();
        for &predecessor in &task.deps {
            successors[predecessor].push(task_id);
        }
    }

    // Kahn traversal with the smallest currently available ID selected first.
    let mut available = std::collections::BinaryHeap::new();
    for (id, &degree) in indegree.iter().enumerate() {
        if degree == 0 {
            available.push(std::cmp::Reverse(id));
        }
    }

    let mut order = Vec::with_capacity(count);
    let mut remaining_indegree = indegree;
    while let Some(std::cmp::Reverse(id)) = available.pop() {
        order.push(id);
        for &child in &successors[id] {
            remaining_indegree[child] -= 1;
            if remaining_indegree[child] == 0 {
                available.push(std::cmp::Reverse(child));
            }
        }
    }

    if order.len() != count {
        let removed = {
            let mut bits = vec![false; count];
            for &id in &order {
                bits[id] = true;
            }
            bits
        };
        let blocked = (0..count).filter(|&id| !removed[id]).collect();
        return Err(Error::Cycle { blocked });
    }

    let mut earliest = vec![Timing { start: 0, finish: 0 }; count];
    let mut makespan = 0u64;
    for &id in &order {
        let start = tasks[id]
            .deps
            .iter()
            .map(|&predecessor| earliest[predecessor].finish)
            .max()
            .unwrap_or(0);
        let finish = start
            .checked_add(tasks[id].duration)
            .ok_or(Error::Overflow { task: id })?;
        earliest[id] = Timing { start, finish };
        makespan = makespan.max(finish);
    }

    if target < makespan {
        return Err(Error::TargetTooSmall { minimum: makespan });
    }

    let mut latest = vec![Timing { start: 0, finish: 0 }; count];
    for &id in order.iter().rev() {
        let finish = successors[id]
            .iter()
            .map(|&child| latest[child].start)
            .min()
            .unwrap_or(target);
        let start = finish - tasks[id].duration;
        latest[id] = Timing { start, finish };
    }

    let slack: Vec<u64> = (0..count)
        .map(|id| latest[id].start - earliest[id].start)
        .collect();
    let baseline = target - makespan;
    let critical = (0..count)
        .filter(|&id| slack[id] == baseline)
        .collect();

    let path = if count == 0 {
        Vec::new()
    } else {
        let sink = (0..count)
            .filter(|&id| successors[id].is_empty() && earliest[id].finish == makespan)
            .max()
            .expect("a nonempty finite DAG has a sink");
        let mut reversed_path = vec![sink];
        let mut current = sink;
        while !tasks[current].deps.is_empty() {
            let predecessor = tasks[current]
                .deps
                .iter()
                .copied()
                .max_by_key(|&id| (earliest[id].finish, id))
                .expect("nonempty dependency list");
            reversed_path.push(predecessor);
            current = predecessor;
        }
        reversed_path.reverse();
        reversed_path
    };

    Ok(Analysis {
        order,
        earliest,
        latest,
        slack,
        critical,
        path,
        makespan,
    })
}