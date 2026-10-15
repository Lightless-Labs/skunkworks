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

    // Validate all dependency entries before doing any graph computation.
    for (task_id, task) in tasks.iter().enumerate() {
        for (position, &dependency) in task.deps.iter().enumerate() {
            if dependency >= n {
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
            if task.deps[..position].contains(&dependency) {
                return Err(Error::DuplicateDependency {
                    task: task_id,
                    position,
                    value: dependency,
                });
            }
        }
    }

    // Build successor lists and indegrees.
    let mut successors = vec![Vec::new(); n];
    let mut indegree = vec![0usize; n];
    for (task_id, task) in tasks.iter().enumerate() {
        indegree[task_id] = task.deps.len();
        for &dependency in &task.deps {
            successors[dependency].push(task_id);
        }
    }

    // Kahn traversal with the smallest currently available ID first.
    let mut available = std::collections::BinaryHeap::new();
    for (task_id, &degree) in indegree.iter().enumerate() {
        if degree == 0 {
            available.push(std::cmp::Reverse(task_id));
        }
    }

    let mut order = Vec::with_capacity(n);
    let mut remaining_indegree = indegree;
    while let Some(std::cmp::Reverse(task_id)) = available.pop() {
        order.push(task_id);
        for &child in &successors[task_id] {
            remaining_indegree[child] -= 1;
            if remaining_indegree[child] == 0 {
                available.push(std::cmp::Reverse(child));
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
        return Err(Error::Cycle {
            blocked: (0..n).filter(|&task_id| !removed[task_id]).collect(),
        });
    }

    // Compute earliest timings in Kahn order.
    let mut earliest: Vec<Option<Timing>> = vec![None; n];
    let mut makespan = 0u64;
    for &task_id in &order {
        let start = tasks[task_id]
            .deps
            .iter()
            .map(|&dependency| earliest[dependency].as_ref().unwrap().finish)
            .max()
            .unwrap_or(0);
        let finish = match start.checked_add(tasks[task_id].duration) {
            Some(finish) => finish,
            None => return Err(Error::Overflow { task: task_id }),
        };
        makespan = makespan.max(finish);
        earliest[task_id] = Some(Timing { start, finish });
    }
    let earliest: Vec<Timing> = earliest.into_iter().map(Option::unwrap).collect();

    if target < makespan {
        return Err(Error::TargetTooSmall { minimum: makespan });
    }

    // Compute latest timings in reverse Kahn order.
    let mut latest: Vec<Option<Timing>> = vec![None; n];
    for &task_id in order.iter().rev() {
        let finish = if successors[task_id].is_empty() {
            target
        } else {
            successors[task_id]
                .iter()
                .map(|&child| latest[child].as_ref().unwrap().start)
                .min()
                .unwrap()
        };
        let start = finish - tasks[task_id].duration;
        latest[task_id] = Some(Timing { start, finish });
    }
    let latest: Vec<Timing> = latest.into_iter().map(Option::unwrap).collect();

    let slack: Vec<u64> = (0..n)
        .map(|task_id| latest[task_id].start - earliest[task_id].start)
        .collect();
    let baseline = target - makespan;
    let critical: Vec<usize> = (0..n)
        .filter(|&task_id| slack[task_id] == baseline)
        .collect();

    // Choose the greatest-ID sink ending at makespan, then follow local
    // greatest-earliest-finish predecessor choices (greatest ID on ties).
    let mut path = Vec::new();
    if n > 0 {
        let mut current = (0..n)
            .filter(|&task_id| successors[task_id].is_empty())
            .filter(|&task_id| earliest[task_id].finish == makespan)
            .max()
            .unwrap();

        loop {
            path.push(current);
            if tasks[current].deps.is_empty() {
                break;
            }
            current = tasks[current]
                .deps
                .iter()
                .copied()
                .max_by_key(|&predecessor| (earliest[predecessor].finish, predecessor))
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

fn main() {}