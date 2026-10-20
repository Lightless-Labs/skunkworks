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
    let n = tasks.len();
    for (task_id, task) in tasks.iter().enumerate() {
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
            if task.deps[..position].contains(&value) {
                return Err(Error::DuplicateDependency {
                    task: task_id,
                    position,
                    value,
                });
            }
        }
    }
    Ok(())
}

fn kahn(tasks: &[Task]) -> Result<(Vec<usize>, Vec<Vec<usize>>), Error> {
    let n = tasks.len();
    let mut children = vec![Vec::new(); n];
    let mut indegree = vec![0usize; n];

    for (task_id, task) in tasks.iter().enumerate() {
        indegree[task_id] = task.deps.len();
        for &predecessor in &task.deps {
            children[predecessor].push(task_id);
        }
    }

    let mut available = BinaryHeap::new();
    for (id, &degree) in indegree.iter().enumerate() {
        if degree == 0 {
            available.push(Reverse(id));
        }
    }

    let mut order = Vec::with_capacity(n);
    while let Some(Reverse(id)) = available.pop() {
        order.push(id);
        for &child in &children[id] {
            indegree[child] -= 1;
            if indegree[child] == 0 {
                available.push(Reverse(child));
            }
        }
    }

    if order.len() != n {
        let removed = {
            let mut seen = vec![false; n];
            for &id in &order {
                seen[id] = true;
            }
            seen
        };
        let blocked = (0..n).filter(|&id| !removed[id]).collect();
        return Err(Error::Cycle { blocked });
    }

    Ok((order, children))
}

pub fn analyze(tasks: &[Task], target: u64) -> Result<Analysis, Error> {
    validate_dependencies(tasks)?;
    let (order, children) = kahn(tasks)?;

    let n = tasks.len();
    let mut earliest = vec![Timing { start: 0, finish: 0 }; n];
    let mut makespan = 0u64;

    for &id in &order {
        let start = tasks[id]
            .deps
            .iter()
            .map(|&pred| earliest[pred].finish)
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

    let mut latest = vec![Timing { start: 0, finish: 0 }; n];
    for &id in order.iter().rev() {
        let finish = if children[id].is_empty() {
            target
        } else {
            children[id]
                .iter()
                .map(|&child| latest[child].start)
                .min()
                .unwrap()
        };
        let start = finish - tasks[id].duration;
        latest[id] = Timing { start, finish };
    }

    let slack: Vec<u64> = (0..n)
        .map(|id| latest[id].start - earliest[id].start)
        .collect();
    let baseline = target - makespan;
    let critical = (0..n).filter(|&id| slack[id] == baseline).collect();

    let sink = (0..n)
        .filter(|&id| children[id].is_empty() && earliest[id].finish == makespan)
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
                .max_by_key(|&&pred| (earliest[pred].finish, pred))
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
