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

fn validate_dependencies(tasks: &[Task]) -> Result<(), Error> {
    let count = tasks.len();

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

    Ok(())
}

fn kahn_order(tasks: &[Task]) -> Result<(Vec<usize>, Vec<Vec<usize>>), Error> {
    let count = tasks.len();
    let mut successors = vec![Vec::new(); count];
    let mut indegree = vec![0usize; count];

    for (task_id, task) in tasks.iter().enumerate() {
        indegree[task_id] = task.deps.len();
        for &predecessor in &task.deps {
            successors[predecessor].push(task_id);
        }
    }

    let mut available = std::collections::BinaryHeap::new();
    for (task_id, &degree) in indegree.iter().enumerate() {
        if degree == 0 {
            available.push(std::cmp::Reverse(task_id));
        }
    }

    let mut order = Vec::with_capacity(count);
    while let Some(std::cmp::Reverse(task_id)) = available.pop() {
        order.push(task_id);
        for &child in &successors[task_id] {
            indegree[child] -= 1;
            if indegree[child] == 0 {
                available.push(std::cmp::Reverse(child));
            }
        }
    }

    if order.len() != count {
        let mut removed = vec![false; count];
        for &task_id in &order {
            removed[task_id] = true;
        }
        let blocked = (0..count).filter(|&id| !removed[id]).collect();
        return Err(Error::Cycle { blocked });
    }

    Ok((order, successors))
}

fn earliest_times(tasks: &[Task], order: &[usize]) -> Result<(Vec<Timing>, u64), Error> {
    let mut earliest: Vec<Option<Timing>> = vec![None; tasks.len()];

    for &task_id in order {
        let start = tasks[task_id]
            .deps
            .iter()
            .filter_map(|&predecessor| earliest[predecessor].as_ref().map(|t| t.finish))
            .max()
            .unwrap_or(0);

        let finish = start
            .checked_add(tasks[task_id].duration)
            .ok_or(Error::Overflow { task: task_id })?;

        earliest[task_id] = Some(Timing { start, finish });
    }

    let earliest: Vec<Timing> = earliest
        .into_iter()
        .map(|timing| timing.expect("all tasks are present in a complete Kahn order"))
        .collect();
    let makespan = earliest.iter().map(|timing| timing.finish).max().unwrap_or(0);

    Ok((earliest, makespan))
}

pub fn analyze(tasks: &[Task], target: u64) -> Result<Analysis, Error> {
    validate_dependencies(tasks)?;

    let (order, successors) = kahn_order(tasks)?;
    let (earliest, makespan) = earliest_times(tasks, &order)?;

    if target < makespan {
        return Err(Error::TargetTooSmall { minimum: makespan });
    }

    let mut latest: Vec<Option<Timing>> = vec![None; tasks.len()];
    for &task_id in order.iter().rev() {
        let finish = if successors[task_id].is_empty() {
            target
        } else {
            successors[task_id]
                .iter()
                .filter_map(|&child| latest[child].as_ref().map(|timing| timing.start))
                .min()
                .expect("every successor follows this task in Kahn order")
        };
        let start = finish - tasks[task_id].duration;
        latest[task_id] = Some(Timing { start, finish });
    }

    let latest: Vec<Timing> = latest
        .into_iter()
        .map(|timing| timing.expect("all tasks are present in a complete Kahn order"))
        .collect();

    let slack: Vec<u64> = latest
        .iter()
        .zip(&earliest)
        .map(|(late, early)| late.start - early.start)
        .collect();

    let baseline = target - makespan;
    let critical: Vec<usize> = slack
        .iter()
        .enumerate()
        .filter_map(|(task_id, &value)| (value == baseline).then_some(task_id))
        .collect();

    let sink = (0..tasks.len())
        .filter(|&task_id| successors[task_id].is_empty())
        .filter(|&task_id| earliest[task_id].finish == makespan)
        .max();

    let mut path = Vec::new();
    if let Some(mut current) = sink {
        loop {
            path.push(current);
            if tasks[current].deps.is_empty() {
                break;
            }
            current = *tasks[current]
                .deps
                .iter()
                .max_by_key(|&&predecessor| {
                    (earliest[predecessor].finish, predecessor)
                })
                .expect("a non-root task has at least one predecessor");
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