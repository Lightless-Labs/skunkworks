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

    // Validate every dependency entry before performing any other processing.
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

    // Build successor lists and indegrees from the validated dependencies.
    let mut successors = vec![Vec::new(); n];
    let mut indegree = vec![0usize; n];
    for (task_id, task) in tasks.iter().enumerate() {
        indegree[task_id] = task.deps.len();
        for &predecessor in &task.deps {
            successors[predecessor].push(task_id);
        }
    }

    // Kahn traversal using the smallest currently available task ID.
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
        let mut removed = vec![false; n];
        for &task_id in &order {
            removed[task_id] = true;
        }
        let blocked = (0..n).filter(|&task_id| !removed[task_id]).collect();
        return Err(Error::Cycle { blocked });
    }

    // Compute earliest timings in Kahn order.
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

    // Compute latest timings in reverse Kahn order.
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

    // Choose the greatest-ID sink finishing at makespan, then walk backward
    // with the specified local predecessor tie-break.
    let mut path = Vec::new();
    if n > 0 {
        let sink = (0..n)
            .filter(|&task_id| successors[task_id].is_empty())
            .filter(|&task_id| earliest[task_id].finish == makespan)
            .max();

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
                    .expect("non-root task has a predecessor");
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

fn main() {}