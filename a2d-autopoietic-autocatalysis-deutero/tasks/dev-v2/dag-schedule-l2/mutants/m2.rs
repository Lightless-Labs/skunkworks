// Fault: Choose smallest sink on equal finish
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Task { pub duration: u64, pub deps: Vec<usize> }
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Timing { pub start: u64, pub finish: u64 }
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Analysis {
    pub order: Vec<usize>, pub earliest: Vec<Timing>, pub latest: Vec<Timing>,
    pub slack: Vec<u64>, pub critical: Vec<usize>, pub path: Vec<usize>, pub makespan: u64,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Error {
    BadDependency { task: usize, position: usize, value: usize },
    SelfDependency { task: usize, position: usize },
    DuplicateDependency { task: usize, position: usize, value: usize },
    Cycle { blocked: Vec<usize> },
    Overflow { task: usize },
    TargetTooSmall { minimum: u64 },
}
fn validate_deps(tasks: &[Task]) -> Result<(), Error> {
    for (task, item) in tasks.iter().enumerate() {
        let mut seen = std::collections::BTreeSet::new();
        for (position, &value) in item.deps.iter().enumerate() {
            if value >= tasks.len() { return Err(Error::BadDependency { task, position, value }); }
            if value == task { return Err(Error::SelfDependency { task, position }); }
            if !seen.insert(value) { return Err(Error::DuplicateDependency { task, position, value }); }
        }
    }
    Ok(())
}
fn topology(tasks: &[Task]) -> Result<Vec<usize>, Error> {
    let mut indegree: Vec<usize> = tasks.iter().map(|t| t.deps.len()).collect();
    let mut children = vec![Vec::new(); tasks.len()];
    for (id, t) in tasks.iter().enumerate() { for &d in &t.deps { children[d].push(id); } }
    let mut ready = std::collections::BTreeSet::new();
    for (i, &degree) in indegree.iter().enumerate() { if degree == 0 { ready.insert(i); } }
    let mut order = Vec::new();
    while let Some(id) = ready.pop_first() {
        order.push(id);
        for &c in &children[id] { indegree[c] -= 1; if indegree[c] == 0 { ready.insert(c); } }
    }
    if order.len() != tasks.len() {
        return Err(Error::Cycle { blocked: (0..tasks.len()).filter(|&i| indegree[i] > 0).collect() });
    }
    Ok(order)
}
fn early_pass(tasks: &[Task], order: &[usize]) -> Result<(Vec<Timing>, u64), Error> {
    let mut times = vec![Timing { start: 0, finish: 0 }; tasks.len()];
    let mut makespan = 0;
    for &id in order {
        let start = tasks[id].deps.iter().map(|&d| times[d].finish).max().unwrap_or(0);
        let finish = start.checked_add(tasks[id].duration).ok_or(Error::Overflow { task: id })?;
        times[id] = Timing { start, finish };
        makespan = makespan.max(finish);
    }
    Ok((times, makespan))
}
pub fn analyze(tasks: &[Task], target: u64) -> Result<Analysis, Error> {
    validate_deps(tasks)?;
    let order = topology(tasks)?;
    let (earliest, makespan) = early_pass(tasks, &order)?;
    if target < makespan { return Err(Error::TargetTooSmall { minimum: makespan }); }
    let mut children = vec![Vec::new(); tasks.len()];
    for (id, t) in tasks.iter().enumerate() { for &d in &t.deps { children[d].push(id); } }
    let mut latest = vec![Timing { start: 0, finish: 0 }; tasks.len()];
    for &id in order.iter().rev() {
        let finish = children[id].iter().map(|&c| latest[c].start).min().unwrap_or(target);
        latest[id] = Timing { start: finish - tasks[id].duration, finish };
    }
    let slack: Vec<u64> = (0..tasks.len()).map(|i| latest[i].start - earliest[i].start).collect();
    let baseline = target - makespan;
    let critical = (0..tasks.len()).filter(|&i| slack[i] == baseline).collect();
    let mut path = Vec::new();
    let sink = (0..tasks.len()).filter(|&i| children[i].is_empty() && earliest[i].finish == makespan).min();
    if let Some(mut id) = sink {
        loop {
            path.push(id);
            if let Some(d) = tasks[id].deps.iter().copied().max_by_key(|&d| (earliest[d].finish, d)) { id = d; }
            else { break; }
        }
        path.reverse();
    }
    Ok(Analysis { order, earliest, latest, slack, critical, path, makespan })
}
