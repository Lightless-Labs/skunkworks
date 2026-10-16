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

    // Validate all dependency entries before doing any other work.
    for (task_id, task) in tasks.iter().enumerate() {
        let mut seen = std::collections::HashSet::new();
        for (position, &dependency) in task.deps.iter().enumerate() {
            if dependency >= count {
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
            if !seen.insert(dependency) {
                return Err(Error::DuplicateDependency {
                    task: task_id,
                    position,
                    value: dependency,
                });
            }
        }
    }

    let mut successors = vec![Vec::new(); count];
    let mut indegree = vec![0usize; count];
    for (task_id, task) in tasks.iter().enumerate() {
        indegree[task_id] = task.deps.len();
        for &dependency in &task.deps {
            successors[dependency].push(task_id);
        }
    }

    let mut available = std::collections::BTreeSet::new();
    for (task_id, &degree) in indegree.iter().enumerate() {
        if degree == 0 {
            available.insert(task_id);
        }
    }

    let mut order = Vec::with_capacity(count);
    let mut remaining_indegree = indegree;
    while let Some(&task_id) = available.iter().next() {
        available.remove(&task_id);
        order.push(task_id);

        for &child in &successors[task_id] {
            remaining_indegree[child] -= 1;
            if remaining_indegree[child] == 0 {
                available.insert(child);
            }
        }
    }

    if order.len() != count {
        let removed: Vec<bool> = {
            let mut flags = vec![false; count];
            for &task_id in &order {
                flags[task_id] = true;
            }
            flags
        };
        let blocked = (0..count).filter(|&task_id| !removed[task_id]).collect();
        return Err(Error::Cycle { blocked });
    }

    let mut earliest = vec![Timing { start: 0, finish: 0 }; count];
    for &task_id in &order {
        let start = tasks[task_id]
            .deps
            .iter()
            .map(|&dependency| earliest[dependency].finish)
            .max()
            .unwrap_or(0);
        let finish = start
            .checked_add(tasks[task_id].duration)
            .ok_or(Error::Overflow { task: task_id })?;
        earliest[task_id] = Timing { start, finish };
    }

    let makespan = earliest
        .iter()
        .map(|timing| timing.finish)
        .max()
        .unwrap_or(0);

    if target < makespan {
        return Err(Error::TargetTooSmall { minimum: makespan });
    }

    let mut latest = vec![Timing { start: 0, finish: 0 }; count];
    for &task_id in order.iter().rev() {
        let finish = if successors[task_id].is_empty() {
            target
        } else {
            successors[task_id]
                .iter()
                .map(|&child| latest[child].start)
                .min()
                .unwrap()
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

    let mut path = Vec::new();
    if count > 0 {
        let mut sink = None;
        for task_id in 0..count {
            if successors[task_id].is_empty() && earliest[task_id].finish == makespan {
                sink = Some(sink.map_or(task_id, |current: usize| current.max(task_id)));
            }
        }

        if let Some(mut current) = sink {
            path.push(current);
            while !tasks[current].deps.is_empty() {
                let predecessor = *tasks[current]
                    .deps
                    .iter()
                    .max_by_key(|&&dependency| (earliest[dependency].finish, dependency))
                    .unwrap();
                current = predecessor;
                path.push(current);
            }
            path.reverse();
        }
    }

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