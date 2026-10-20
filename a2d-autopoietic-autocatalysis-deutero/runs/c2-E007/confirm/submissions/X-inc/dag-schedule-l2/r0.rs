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

    // Validate every dependency before doing any graph computation.
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

    let mut successors = vec![Vec::new(); count];
    let mut indegree = vec![0usize; count];
    for (task_id, task) in tasks.iter().enumerate() {
        indegree[task_id] = task.deps.len();
        for &predecessor in &task.deps {
            successors[predecessor].push(task_id);
        }
    }

    // Use a min-heap encoded as Reverse so newly available smaller IDs
    // take precedence over IDs already in the queue.
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

    // Earliest-time pass in Kahn order.
    let mut earliest: Vec<Option<Timing>> = vec![None; count];
    for &id in &order {
        let start = tasks[id]
            .deps
            .iter()
            .map(|&predecessor| earliest[predecessor].as_ref().unwrap().finish)
            .max()
            .unwrap_or(0);
        let finish = start
            .checked_add(tasks[id].duration)
            .ok_or(Error::Overflow { task: id })?;
        earliest[id] = Some(Timing { start, finish });
    }

    let earliest: Vec<Timing> = earliest.into_iter().map(Option::unwrap).collect();
    let makespan = earliest.iter().map(|timing| timing.finish).max().unwrap_or(0);

    if target < makespan {
        return Err(Error::TargetTooSmall { minimum: makespan });
    }

    // Latest-time pass in reverse Kahn order.
    let mut latest: Vec<Option<Timing>> = vec![None; count];
    for &id in order.iter().rev() {
        let finish = if successors[id].is_empty() {
            target
        } else {
            successors[id]
                .iter()
                .map(|&child| latest[child].as_ref().unwrap().start)
                .min()
                .unwrap()
        };
        let start = finish - tasks[id].duration;
        latest[id] = Some(Timing { start, finish });
    }
    let latest: Vec<Timing> = latest.into_iter().map(Option::unwrap).collect();

    let slack: Vec<u64> = (0..count)
        .map(|id| latest[id].start - earliest[id].start)
        .collect();
    let baseline = target - makespan;
    let critical = (0..count).filter(|&id| slack[id] == baseline).collect();

    // Choose the greatest-ID sink attaining the makespan.
    let sink = (0..count)
        .filter(|&id| successors[id].is_empty() && earliest[id].finish == makespan)
        .max();

    let mut path = Vec::new();
    if let Some(mut id) = sink {
        loop {
            path.push(id);
            if tasks[id].deps.is_empty() {
                break;
            }
            id = *tasks[id]
                .deps
                .iter()
                .max_by_key(|&&predecessor| (earliest[predecessor].finish, predecessor))
                .unwrap();
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