use std::collections::VecDeque;
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Job { pub id: u32, pub arrival: u32, pub ticks: u32 }
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Run { pub id: u32, pub start: u64, pub end: u64, pub finished: bool }
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Schedule { pub runs: Vec<Run>, pub idle: Vec<(u64, u64)>, pub completion: Vec<(u32, u64)>, pub end: u64 }
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Error { Quantum, SwitchCost, TooManyJobs, DuplicateId { index: usize }, Arrival { index: usize }, Ticks { index: usize } }


pub fn schedule(jobs: &[Job], quantum: u32, switch_cost: u32) -> Result<Schedule, Error> {
    if quantum == 0 || quantum > 1000 { return Err(Error::Quantum); }
    let quanta = &[quantum];
    if switch_cost > 1000 { return Err(Error::SwitchCost); }
    if jobs.len() > 64 { return Err(Error::TooManyJobs); }
    for (i, job) in jobs.iter().enumerate() {
        if jobs[..i].iter().any(|j| j.id == job.id) { return Err(Error::DuplicateId { index: i }); }
        if job.arrival > 1000 { return Err(Error::Arrival { index: i }); }
        if job.ticks > 1000 { return Err(Error::Ticks { index: i }); }
    }
    let n = jobs.len();
    let mut queues: Vec<VecDeque<usize>> = vec![VecDeque::new(); quanta.len()];
    // Event keys: timestamp, kind (external before wake), original input index.
    let mut events: Vec<(u64, u8, usize)> = jobs.iter().enumerate()
        .map(|(i,j)| (j.arrival as u64, 0, i)).collect();
    let mut done = vec![0u64; n];
    let mut finished_count = 0;
    let mut now = 0u64;
    let mut last = None;
    let mut runs = Vec::new();
    let mut idle: Vec<(u64,u64)> = Vec::new();
    let mut levels = vec![0usize; n];
    let mut remaining: Vec<u32> = jobs.iter().map(|j| j.ticks).collect();
    let admit = |until: u64, events: &mut Vec<(u64,u8,usize)>,
                 queues: &mut Vec<VecDeque<usize>>, done: &mut Vec<u64>,
                 finished_count: &mut usize, levels: &mut Vec<usize>| {
        events.sort_unstable();
        while !events.is_empty() && events[0].0 <= until {
            let (at, kind, i) = events.remove(0);
            let _ = kind;
            if jobs[i].ticks == 0 { done[i] = at; *finished_count += 1; }
            else { queues[levels[i]].push_back(i); }
        }
    };
    while finished_count < n {
        admit(now, &mut events, &mut queues, &mut done, &mut finished_count, &mut levels);
        if finished_count == n { break; }
        let Some(level) = queues.iter().position(|q| !q.is_empty()) else {
            let next = events.iter().map(|e| e.0).min().unwrap();
            if next > now {
                if let Some(prev) = idle.last_mut().filter(|p| p.1 == now) { prev.1 = next; }
                else { idle.push((now, next)); }
                now = next;
            }
            continue;
        };
        let i = queues[level].pop_front().unwrap();
        if last.is_some() && last != Some(jobs[i].id) { now += switch_cost as u64; }
        admit(now, &mut events, &mut queues, &mut done, &mut finished_count, &mut levels);
        let start = now;
        let used = remaining[i].min(quanta[level]);
        remaining[i] -= used;
        now += used as u64;
        let finished = remaining[i] == 0;
        runs.push(Run { id: jobs[i].id, start, end: now, finished });
        last = Some(jobs[i].id);
        admit(now, &mut events, &mut queues, &mut done, &mut finished_count, &mut levels);
        if finished { done[i] = now; finished_count += 1; }
        else {
            queues[levels[i]].push_back(i);
        }
    }
    let completion = jobs.iter().enumerate().map(|(i,j)| (j.id, done[i])).collect();
    Ok(Schedule { runs, idle, completion, end: now })
}
