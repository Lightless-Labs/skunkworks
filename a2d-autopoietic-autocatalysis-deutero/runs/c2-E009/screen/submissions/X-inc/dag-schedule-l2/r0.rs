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

    // Validate every dependency before performing any graph or timing work.
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

    let mut successors = vec![Vec::new(); count];
    let mut indegree = vec![0usize; count];
    for (task_id, task) in tasks.iter().enumerate() {
        indegree[task_id] = task.deps.len();
        for &predecessor in &task.deps {
            successors[predecessor].push(task_id);
        }
    }

    // Kahn traversal with the smallest currently available ID first.
    let mut available = std::collections::BTreeSet::new();
    for (task_id, &degree) in indegree.iter().enumerate() {
        if degree == 0 {
            available.insert(task_id);
        }
    }

    let mut order = Vec::with_capacity(count);
    while let Some(&task_id) = available.iter().next() {
        available.remove(&task_id);
        order.push(task_id);

        for &child in &successors[task_id] {
            indegree[child] -= 1;
            if indegree[child] == 0 {
                available.insert(child);
            }
        }
    }

    if order.len() != count {
        let mut blocked = Vec::new();
        for task_id in 0..count {
            if indegree[task_id] != 0 {
                blocked.push(task_id);
            }
        }
        return Err(Error::Cycle { blocked });
    }

    // Compute all earliest times in Kahn order.
    let mut earliest = vec![Timing { start: 0, finish: 0 }; count];
    let mut makespan = 0u64;

    for &task_id in &order {
        let start = tasks[task_id]
            .deps
            .iter()
            .map(|&predecessor| earliest[predecessor].finish)
            .max()
            .unwrap_or(0);

        let finish = match start.checked_add(tasks[task_id].duration) {
            Some(finish) => finish,
            None => return Err(Error::Overflow { task: task_id }),
        };

        earliest[task_id] = Timing { start, finish };
        makespan = makespan.max(finish);
    }

    if target < makespan {
        return Err(Error::TargetTooSmall { minimum: makespan });
    }

    // Compute latest times in reverse Kahn order.
    let mut latest = vec![Timing { start: 0, finish: 0 }; count];
    for &task_id in order.iter().rev() {
        let finish = if successors[task_id].is_empty() {
            target
        } else {
            successors[task_id]
                .iter()
                .map(|&child| latest[child].start)
                .min()
                .unwrap_or(target)
        };
        let start = finish - tasks[task_id].duration;
        latest[task_id] = Timing { start, finish };
    }

    let slack: Vec<u64> = (0..count)
        .map(|task_id| latest[task_id].start - earliest[task_id].start)
        .collect();

    let baseline = target - makespan;
    let critical = (0..count)
        .filter(|&task_id| slack[task_id] == baseline)
        .collect();

    // Pick the greatest-ID sink attaining the makespan, then follow the
    // greatest earliest-finish predecessor, breaking ties by greatest ID.
    let path = if count == 0 {
        Vec::new()
    } else {
        let mut current = (0..count)
            .filter(|&task_id| successors[task_id].is_empty())
            .filter(|&task_id| earliest[task_id].finish == makespan)
            .max()
            .expect("a nonempty finite DAG has a sink");

        let mut reversed_path = vec![current];
        while !tasks[current].deps.is_empty() {
            current = tasks[current]
                .deps
                .iter()
                .copied()
                .max_by_key(|&predecessor| (earliest[predecessor].finish, predecessor))
                .expect("nonempty dependency list has a predecessor");
            reversed_path.push(current);
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