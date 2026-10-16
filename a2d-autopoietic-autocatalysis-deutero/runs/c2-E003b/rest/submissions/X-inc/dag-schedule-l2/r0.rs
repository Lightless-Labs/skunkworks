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

    // Validate every dependency before computing anything else.
    for (task_id, task) in tasks.iter().enumerate() {
        let mut seen = std::collections::HashSet::new();
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
            if !seen.insert(value) {
                return Err(Error::DuplicateDependency {
                    task: task_id,
                    position,
                    value,
                });
            }
        }
    }

    // Build predecessor counts and successor adjacency lists.
    let mut indegree = vec![0usize; n];
    let mut successors = vec![Vec::new(); n];
    for (task_id, task) in tasks.iter().enumerate() {
        indegree[task_id] = task.deps.len();
        for &predecessor in &task.deps {
            successors[predecessor].push(task_id);
        }
    }

    // Kahn traversal, always removing the smallest currently available ID.
    let mut available = std::collections::BinaryHeap::new();
    for (id, &degree) in indegree.iter().enumerate() {
        if degree == 0 {
            available.push(std::cmp::Reverse(id));
        }
    }

    let mut order = Vec::with_capacity(n);
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

    if order.len() != n {
        let removed = {
            let mut set = vec![false; n];
            for &id in &order {
                set[id] = true;
            }
            set
        };
        return Err(Error::Cycle {
            blocked: (0..n).filter(|&id| !removed[id]).collect(),
        });
    }

    // Compute earliest times in Kahn order.
    let mut earliest: Vec<Option<Timing>> = vec![None; n];
    let mut makespan = 0u64;
    for &id in &order {
        let start = tasks[id]
            .deps
            .iter()
            .map(|&pred| earliest[pred].as_ref().unwrap().finish)
            .max()
            .unwrap_or(0);
        let finish = start
            .checked_add(tasks[id].duration)
            .ok_or(Error::Overflow { task: id })?;
        makespan = makespan.max(finish);
        earliest[id] = Some(Timing { start, finish });
    }
    let earliest: Vec<Timing> = earliest.into_iter().map(Option::unwrap).collect();

    if target < makespan {
        return Err(Error::TargetTooSmall { minimum: makespan });
    }

    // Compute latest times in reverse Kahn order.
    let mut latest: Vec<Option<Timing>> = vec![None; n];
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

    let slack: Vec<u64> = (0..n)
        .map(|id| latest[id].start - earliest[id].start)
        .collect();
    let baseline = target - makespan;
    let critical: Vec<usize> = (0..n).filter(|&id| slack[id] == baseline).collect();

    // Select the greatest-ID sink finishing at makespan, then follow the
    // specified local predecessor tie-break backwards.
    let mut sink = None;
    for id in 0..n {
        if successors[id].is_empty() && earliest[id].finish == makespan {
            sink = Some(id);
        }
    }

    let mut path = Vec::new();
    if let Some(mut current) = sink {
        loop {
            path.push(current);
            if tasks[current].deps.is_empty() {
                break;
            }
            let mut predecessor = tasks[current].deps[0];
            for &candidate in &tasks[current].deps[1..] {
                if earliest[candidate].finish > earliest[predecessor].finish
                    || (earliest[candidate].finish == earliest[predecessor].finish
                        && candidate > predecessor)
                {
                    predecessor = candidate;
                }
            }
            current = predecessor;
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