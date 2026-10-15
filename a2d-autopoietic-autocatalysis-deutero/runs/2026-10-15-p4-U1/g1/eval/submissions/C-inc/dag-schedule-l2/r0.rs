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
        let mut seen = vec![false; count];

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
            if seen[dependency] {
                return Err(Error::DuplicateDependency {
                    task: task_id,
                    position,
                    value: dependency,
                });
            }
            seen[dependency] = true;
        }
    }

    Ok(())
}

fn insert_available(available: &mut Vec<usize>, id: usize) {
    let position = available.binary_search(&id).unwrap_or_else(|position| position);
    available.insert(position, id);
}

pub fn analyze(tasks: &[Task], target: u64) -> Result<Analysis, Error> {
    validate_dependencies(tasks)?;

    let count = tasks.len();
    let mut indegree = vec![0usize; count];
    let mut successors = vec![Vec::new(); count];

    for (task_id, task) in tasks.iter().enumerate() {
        indegree[task_id] = task.deps.len();
        for &dependency in &task.deps {
            successors[dependency].push(task_id);
        }
    }

    let mut available = Vec::new();
    for (id, &degree) in indegree.iter().enumerate() {
        if degree == 0 {
            insert_available(&mut available, id);
        }
    }

    let mut order = Vec::with_capacity(count);
    let mut remaining_indegree = indegree;
    while !available.is_empty() {
        let id = available.remove(0);
        order.push(id);

        for &child in &successors[id] {
            remaining_indegree[child] -= 1;
            if remaining_indegree[child] == 0 {
                insert_available(&mut available, child);
            }
        }
    }

    if order.len() != count {
        let mut removed = vec![false; count];
        for &id in &order {
            removed[id] = true;
        }
        let blocked = (0..count).filter(|&id| !removed[id]).collect();
        return Err(Error::Cycle { blocked });
    }

    let mut earliest = vec![Timing { start: 0, finish: 0 }; count];
    for &id in &order {
        let start = tasks[id]
            .deps
            .iter()
            .map(|&dependency| earliest[dependency].finish)
            .max()
            .unwrap_or(0);

        let Some(finish) = start.checked_add(tasks[id].duration) else {
            return Err(Error::Overflow { task: id });
        };
        earliest[id] = Timing { start, finish };
    }

    let makespan = earliest.iter().map(|timing| timing.finish).max().unwrap_or(0);
    if target < makespan {
        return Err(Error::TargetTooSmall { minimum: makespan });
    }

    let mut latest = vec![Timing { start: 0, finish: 0 }; count];
    for &id in order.iter().rev() {
        let finish = if successors[id].is_empty() {
            target
        } else {
            successors[id]
                .iter()
                .map(|&child| latest[child].start)
                .min()
                .unwrap_or(target)
        };
        let start = finish - tasks[id].duration;
        latest[id] = Timing { start, finish };
    }

    let slack: Vec<u64> = (0..count)
        .map(|id| latest[id].start - earliest[id].start)
        .collect();
    let baseline = target - makespan;
    let critical = (0..count)
        .filter(|&id| slack[id] == baseline)
        .collect();

    let mut path = Vec::new();
    if count > 0 {
        let mut current = (0..count)
            .filter(|&id| successors[id].is_empty() && earliest[id].finish == makespan)
            .max()
            .unwrap_or(0);

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