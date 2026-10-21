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

    let mut indegree: Vec<usize> = tasks.iter().map(|task| task.deps.len()).collect();
    let mut children = vec![Vec::new(); count];
    for (task_id, task) in tasks.iter().enumerate() {
        for &predecessor in &task.deps {
            children[predecessor].push(task_id);
        }
    }

    // A min-heap (via Reverse) keeps the smallest available ID first.
    let mut available = std::collections::BinaryHeap::new();
    for (task_id, &degree) in indegree.iter().enumerate() {
        if degree == 0 {
            available.push(std::cmp::Reverse(task_id));
        }
    }

    let mut order = Vec::with_capacity(count);
    while let Some(std::cmp::Reverse(task_id)) = available.pop() {
        order.push(task_id);
        for &child in &children[task_id] {
            indegree[child] -= 1;
            if indegree[child] == 0 {
                available.push(std::cmp::Reverse(child));
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
        return Err(Error::Cycle {
            blocked: (0..count).filter(|&task_id| !removed[task_id]).collect(),
        });
    }

    let mut earliest: Vec<Option<Timing>> = vec![None; count];
    let mut makespan = 0;

    for &task_id in &order {
        let start = tasks[task_id]
            .deps
            .iter()
            .map(|&predecessor| earliest[predecessor].as_ref().unwrap().finish)
            .max()
            .unwrap_or(0);
        let finish = start
            .checked_add(tasks[task_id].duration)
            .ok_or(Error::Overflow { task: task_id })?;
        earliest[task_id] = Some(Timing { start, finish });
        makespan = makespan.max(finish);
    }

    if target < makespan {
        return Err(Error::TargetTooSmall { minimum: makespan });
    }

    let mut latest: Vec<Option<Timing>> = vec![None; count];
    for &task_id in order.iter().rev() {
        let finish = if children[task_id].is_empty() {
            target
        } else {
            children[task_id]
                .iter()
                .map(|&child| latest[child].as_ref().unwrap().start)
                .min()
                .unwrap()
        };
        let start = finish - tasks[task_id].duration;
        latest[task_id] = Some(Timing { start, finish });
    }

    let earliest: Vec<Timing> = earliest.into_iter().map(Option::unwrap).collect();
    let latest: Vec<Timing> = latest.into_iter().map(Option::unwrap).collect();
    let slack: Vec<u64> = (0..count)
        .map(|task_id| latest[task_id].start - earliest[task_id].start)
        .collect();
    let baseline = target - makespan;
    let critical: Vec<usize> = (0..count)
        .filter(|&task_id| slack[task_id] == baseline)
        .collect();

    let mut path = Vec::new();
    if count != 0 {
        let mut current = (0..count)
            .filter(|&task_id| children[task_id].is_empty())
            .filter(|&task_id| earliest[task_id].finish == makespan)
            .max()
            .unwrap();

        loop {
            path.push(current);
            let predecessor = tasks[current]
                .deps
                .iter()
                .copied()
                .max_by_key(|&task_id| (earliest[task_id].finish, task_id));
            match predecessor {
                Some(task_id) => current = task_id,
                None => break,
            }
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