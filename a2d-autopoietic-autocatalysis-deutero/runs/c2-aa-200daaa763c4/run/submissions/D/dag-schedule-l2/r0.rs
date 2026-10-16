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

    let mut successors = vec![Vec::new(); n];
    let mut indegree = vec![0usize; n];
    for (task_id, task) in tasks.iter().enumerate() {
        indegree[task_id] = task.deps.len();
        for &predecessor in &task.deps {
            successors[predecessor].push(task_id);
        }
    }

    // Sorted available IDs; insertion keeps the smallest currently available first.
    let mut available = Vec::new();
    for (id, &degree) in indegree.iter().enumerate() {
        if degree == 0 {
            available.push(id);
        }
    }

    let mut order = Vec::with_capacity(n);
    let mut earliest_starts = vec![0u64; n];
    let mut earliest_finishes = vec![0u64; n];

    while !available.is_empty() {
        let id = available.remove(0);
        order.push(id);

        let finish = earliest_starts[id]
            .checked_add(tasks[id].duration)
            .ok_or(Error::Overflow { task: id })?;
        earliest_finishes[id] = finish;

        for &child in &successors[id] {
            if finish > earliest_starts[child] {
                earliest_starts[child] = finish;
            }
            indegree[child] -= 1;
            if indegree[child] == 0 {
                let insertion = available.binary_search(&child).unwrap_or_else(|i| i);
                available.insert(insertion, child);
            }
        }
    }

    if order.len() != n {
        let mut removed = vec![false; n];
        for &id in &order {
            removed[id] = true;
        }
        return Err(Error::Cycle {
            blocked: (0..n).filter(|&id| !removed[id]).collect(),
        });
    }

    let earliest: Vec<Timing> = (0..n)
        .map(|id| Timing {
            start: earliest_starts[id],
            finish: earliest_finishes[id],
        })
        .collect();

    let makespan = earliest_finishes.iter().copied().max().unwrap_or(0);
    if target < makespan {
        return Err(Error::TargetTooSmall { minimum: makespan });
    }

    let mut latest_starts = vec![0u64; n];
    let mut latest_finishes = vec![0u64; n];

    for &id in order.iter().rev() {
        let latest_finish = if successors[id].is_empty() {
            target
        } else {
            successors[id]
                .iter()
                .map(|&child| latest_starts[child])
                .min()
                .unwrap()
        };
        let latest_start = latest_finish - tasks[id].duration;
        latest_finishes[id] = latest_finish;
        latest_starts[id] = latest_start;
    }

    let latest: Vec<Timing> = (0..n)
        .map(|id| Timing {
            start: latest_starts[id],
            finish: latest_finishes[id],
        })
        .collect();

    let slack: Vec<u64> = (0..n)
        .map(|id| latest_starts[id] - earliest_starts[id])
        .collect();

    let baseline = target - makespan;
    let critical = (0..n)
        .filter(|&id| slack[id] == baseline)
        .collect();

    let mut path = Vec::new();
    if n != 0 {
        let mut sink = None;
        for id in 0..n {
            if successors[id].is_empty() && earliest_finishes[id] == makespan {
                sink = Some(id);
            }
        }

        let mut current = sink.unwrap();
        loop {
            path.push(current);
            if tasks[current].deps.is_empty() {
                break;
            }

            let mut best = tasks[current].deps[0];
            for &predecessor in &tasks[current].deps[1..] {
                if earliest_finishes[predecessor] > earliest_finishes[best]
                    || (earliest_finishes[predecessor] == earliest_finishes[best]
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