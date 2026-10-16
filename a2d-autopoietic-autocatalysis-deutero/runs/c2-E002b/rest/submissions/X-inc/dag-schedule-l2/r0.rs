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
    let n = tasks.len();

    // Validate every dependency entry before building or traversing the graph.
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

    // Build successor lists and indegrees.
    let mut successors = vec![Vec::new(); n];
    let mut indegree = vec![0usize; n];
    for (task_id, task) in tasks.iter().enumerate() {
        indegree[task_id] = task.deps.len();
        for &predecessor in &task.deps {
            successors[predecessor].push(task_id);
        }
    }

    // Kahn traversal with the smallest currently available task first.
    let mut available = std::collections::BTreeSet::new();
    for (task_id, &degree) in indegree.iter().enumerate() {
        if degree == 0 {
            available.insert(task_id);
        }
    }

    let mut order = Vec::with_capacity(n);
    let mut remaining_indegree = indegree;
    while let Some(task_id) = available.pop_first() {
        order.push(task_id);
        for &child in &successors[task_id] {
            remaining_indegree[child] -= 1;
            if remaining_indegree[child] == 0 {
                available.insert(child);
            }
        }
    }

    if order.len() != n {
        let removed: Vec<bool> = {
            let mut flags = vec![false; n];
            for &task_id in &order {
                flags[task_id] = true;
            }
            flags
        };
        let blocked = (0..n).filter(|&task_id| !removed[task_id]).collect();
        return Err(Error::Cycle { blocked });
    }

    // Earliest times, evaluated in Kahn order.
    let mut earliest = vec![Timing { start: 0, finish: 0 }; n];
    for &task_id in &order {
        let start = tasks[task_id]
            .deps
            .iter()
            .map(|&predecessor| earliest[predecessor].finish)
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

    // Latest times, evaluated from sinks backward through reverse Kahn order.
    let mut latest = vec![Timing { start: 0, finish: 0 }; n];
    for &task_id in order.iter().rev() {
        let finish = successors[task_id]
            .iter()
            .map(|&child| latest[child].start)
            .min()
            .unwrap_or(target);
        let start = finish - tasks[task_id].duration;
        latest[task_id] = Timing { start, finish };
    }

    let slack: Vec<u64> = (0..n)
        .map(|task_id| latest[task_id].start - earliest[task_id].start)
        .collect();
    let baseline = target - makespan;
    let critical = (0..n)
        .filter(|&task_id| slack[task_id] == baseline)
        .collect();

    // Pick the greatest-ID sink attaining the makespan.
    let mut sink = None;
    for task_id in 0..n {
        if successors[task_id].is_empty()
            && earliest[task_id].finish == makespan
        {
            sink = Some(task_id);
        }
    }

    let mut path = Vec::new();
    if let Some(mut current) = sink {
        loop {
            path.push(current);
            if tasks[current].deps.is_empty() {
                break;
            }
            let mut best = tasks[current].deps[0];
            for &predecessor in &tasks[current].deps[1..] {
                if earliest[predecessor].finish > earliest[best].finish
                    || (earliest[predecessor].finish == earliest[best].finish
                        && predecessor > best)
                {
                    best = predecessor;
                }
            }
            current = best;
        }
        path.reverse();
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