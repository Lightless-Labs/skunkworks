#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Task { pub duration: u64, pub deps: Vec<usize> }
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Timing { pub start: u64, pub finish: u64 }
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Plan { pub order: Vec<usize>, pub times: Vec<Timing>, pub makespan: u64 }
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Error {
    BadDependency { task: usize, position: usize, value: usize },
    SelfDependency { task: usize, position: usize },
    DuplicateDependency { task: usize, position: usize, value: usize },
    Cycle { blocked: Vec<usize> },
    Overflow { task: usize },
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
pub fn earliest(tasks: &[Task]) -> Result<Plan, Error> {
    validate_deps(tasks)?;
    let order = topology(tasks)?;
    let (times, makespan) = early_pass(tasks, &order)?;
    Ok(Plan { order, times, makespan })
}
