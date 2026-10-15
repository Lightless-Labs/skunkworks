// Fault: Descending ID rather than ascending ID at equal rank
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Task { pub duration: u64, pub deps: Vec<usize>, pub release: u64, pub priority: i32 }
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Run { pub start: u64, pub finish: u64, pub worker: usize }
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EventKind { Start, Finish }
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Event { pub time: u64, pub task: usize, pub worker: usize, pub kind: EventKind }
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Schedule { pub runs: Vec<Run>, pub makespan: u64, pub events: Vec<Event> }
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Stage { Tail, Finish, Turnaround }
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Error {
    BadDependency { task: usize, position: usize, value: usize },
    SelfDependency { task: usize, position: usize },
    DuplicateDependency { task: usize, position: usize, value: usize },
    Cycle { blocked: Vec<usize> },
    Workers { count: usize },
    Overflow { task: usize, stage: Stage },
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
fn tails(tasks: &[Task], order: &[usize]) -> Result<Vec<u64>, Error> {
    let mut children = vec![Vec::new(); tasks.len()];
    for (id, t) in tasks.iter().enumerate() { for &d in &t.deps { children[d].push(id); } }
    let mut tail = vec![0u64; tasks.len()];
    for &id in order.iter().rev() {
        let rest = children[id].iter().map(|&c| tail[c]).max().unwrap_or(0);
        tail[id] = tasks[id].duration.checked_add(rest).ok_or(Error::Overflow { task: id, stage: Stage::Tail })?;
    }
    Ok(tail)
}
pub fn schedule(tasks: &[Task], workers: usize, turnaround: u64) -> Result<Schedule, Error> {
    validate_deps(tasks)?;
    if workers == 0 || workers > 64 { return Err(Error::Workers { count: workers }); }
    let order = topology(tasks)?;
    let tail = tails(tasks, &order)?;
    let n = tasks.len();
    let mut runs = vec![Run { start: 0, finish: 0, worker: 0 }; n];
    let mut state = vec![0u8; n];
    let mut free = vec![0u64; workers];
    let mut now = 0u64;
    let mut done = 0usize;
    let mut events = Vec::new();
    while done < n {
        for id in 0..n {
            if state[id] == 1 && runs[id].finish == now {
                state[id] = 2;
                done += 1;
                events.push(Event { time: now, task: id, worker: runs[id].worker, kind: EventKind::Finish });
            }
        }
        if done == n { break; }
        let candidate = (0..n).filter(|&id| {
            state[id] == 0 && tasks[id].release <= now && tasks[id].deps.iter().all(|&d| state[d] == 2)
        }).max_by_key(|&id| (tail[id], tasks[id].priority, id));
        if let Some(id) = candidate {
            let worker = (0..workers).find(|&w| free[w] <= now);
            if let Some(worker) = worker {
                let finish = now.checked_add(tasks[id].duration).ok_or(Error::Overflow { task: id, stage: Stage::Finish })?;
                let available = finish.checked_add(turnaround).ok_or(Error::Overflow { task: id, stage: Stage::Turnaround })?;
                runs[id] = Run { start: now, finish, worker };
                free[worker] = available;
                state[id] = 1;
                events.push(Event { time: now, task: id, worker, kind: EventKind::Start });
                continue;
            }
        }
        let mut next: Option<u64> = None;
        let mut consider = |t: u64| { if t > now { next = Some(next.map_or(t, |old| old.min(t))); } };
        for id in 0..n {
            if state[id] == 1 { consider(runs[id].finish); }
            if state[id] == 0 { consider(tasks[id].release); }
        }
        for &t in &free { consider(t); }
        now = next.expect("a valid DAG with feasible individual resource demands can progress");
    }
    let makespan = runs.iter().map(|r| r.finish).max().unwrap_or(0);
    Ok(Schedule { runs, makespan, events })
}
