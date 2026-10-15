// Fault: Hold resources and dependencies until worker cooldown ends
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Task { pub duration: u64, pub deps: Vec<usize>, pub release: u64, pub priority: i32, pub needs: Vec<u32> }
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Run { pub start: u64, pub finish: u64, pub worker: usize }
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Schedule { pub runs: Vec<Run>, pub makespan: u64, pub trace: String }
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
    ResourceShape { task: usize, expected: usize, actual: usize },
    ResourceExceeded { task: usize, resource: usize, need: u32, capacity: u32 },
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
fn record(trace: &mut String, now: u64, kind: &str, id: usize, worker: usize, used: &[u32]) {
    let snapshot = if used.is_empty() { "-".to_string() }
        else { used.iter().map(u32::to_string).collect::<Vec<_>>().join(",") };
    trace.push_str(&format!("{}|{}|{}|{}|{}\n", now, kind, id, worker, snapshot));
}
pub fn schedule(tasks: &[Task], cooldowns: &[u64], capacity: &[u32]) -> Result<Schedule, Error> {
    validate_deps(tasks)?;
    let workers = cooldowns.len();
    if workers == 0 || workers > 64 { return Err(Error::Workers { count: workers }); }
    for (task, t) in tasks.iter().enumerate() {
        if t.needs.len() != capacity.len() {
            return Err(Error::ResourceShape { task, expected: capacity.len(), actual: t.needs.len() });
        }
        for (resource, (&need, &cap)) in t.needs.iter().zip(capacity).enumerate() {
            if need > cap { return Err(Error::ResourceExceeded { task, resource, need, capacity: cap }); }
        }
    }
    let order = topology(tasks)?;
    let tail = tails(tasks, &order)?;
    let n = tasks.len();
    let mut runs = vec![Run { start: 0, finish: 0, worker: 0 }; n];
    let mut state = vec![0u8; n];
    let mut free = vec![0u64; workers];
    let mut now = 0u64;
    let mut done = 0usize;
    let mut used = vec![0u32; capacity.len()];
    let mut trace = format!("workers={};resources={}\n", workers, capacity.len());
    while done < n {
        for id in 0..n {
            if state[id] == 1 && free[runs[id].worker] == now {
                state[id] = 2;
                done += 1;
                for r in 0..used.len() { used[r] -= tasks[id].needs[r]; }
                record(&mut trace, now, "end", id, runs[id].worker, &used);
            }
        }
        if done == n { break; }
        let candidate = (0..n).filter(|&id| {
            state[id] == 0 && tasks[id].release <= now && tasks[id].deps.iter().all(|&d| state[d] == 2)
                && (0..used.len()).all(|r| tasks[id].needs[r] <= capacity[r] - used[r])
        }).max_by_key(|&id| (tail[id], tasks[id].priority, std::cmp::Reverse(id)));
        if let Some(id) = candidate {
            let worker = (0..workers).map(|offset| (id % workers + offset) % workers).find(|&w| free[w] <= now);
            if let Some(worker) = worker {
                let finish = now.checked_add(tasks[id].duration).ok_or(Error::Overflow { task: id, stage: Stage::Finish })?;
                let available = finish.checked_add(cooldowns[worker]).ok_or(Error::Overflow { task: id, stage: Stage::Turnaround })?;
                runs[id] = Run { start: now, finish, worker };
                free[worker] = available;
                state[id] = 1;
                for r in 0..used.len() { used[r] += tasks[id].needs[r]; }
                record(&mut trace, now, "start", id, worker, &used);
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
    trace.push_str(&format!("makespan={}\n", makespan));
    Ok(Schedule { runs, makespan, trace })
}
