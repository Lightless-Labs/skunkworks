use std::collections::VecDeque;
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Job { pub id: u32, pub arrival: u32, pub bursts: Vec<u32>, pub level: usize }
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Run { pub id: u32, pub start: u64, pub end: u64, pub finished: bool, pub level: usize }
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Schedule { pub runs: Vec<Run>, pub idle: Vec<(u64, u64)>, pub stats: Vec<Stat>, pub mean_waiting_x100: u64, pub mean_turnaround_x100: u64, pub end: u64 }
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Error { LevelCount, Quantum { level: usize }, SwitchCost, TooManyJobs, DuplicateId { index: usize }, Arrival { index: usize }, InitialLevel { index: usize }, BurstCount { index: usize }, Burst { index: usize, position: usize } }
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Stat { pub id: u32, pub completion: u64, pub turnaround: u64, pub waiting: u64 }


pub fn schedule(jobs: &[Job], quanta: &[u32], switch_cost: u32) -> Result<Schedule, Error> {
    if quanta.is_empty() || quanta.len() > 8 { return Err(Error::LevelCount); }
    for (level, &q) in quanta.iter().enumerate() {
        if q == 0 || q > 1000 { return Err(Error::Quantum { level }); }
    }
    if switch_cost > 1000 { return Err(Error::SwitchCost); }
    if jobs.len() > 64 { return Err(Error::TooManyJobs); }
    for (i, job) in jobs.iter().enumerate() {
        if jobs[..i].iter().any(|j| j.id == job.id) { return Err(Error::DuplicateId { index: i }); }
        if job.arrival > 1000 { return Err(Error::Arrival { index: i }); }
        if job.level >= quanta.len() { return Err(Error::InitialLevel { index: i }); }
        if job.bursts.is_empty() || job.bursts.len() > 31 || job.bursts.len() % 2 == 0 {
            return Err(Error::BurstCount { index: i });
        }
        for (position, &v) in job.bursts.iter().enumerate() {
            if v > 1000 || (position % 2 == 0 && v == 0) {
                return Err(Error::Burst { index: i, position });
            }
        }
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
    let mut levels: Vec<usize> = jobs.iter().map(|j| j.level).collect();
    let mut remaining: Vec<u32> = jobs.iter().map(|j| j.bursts[0]).collect();
    let mut positions = vec![0usize; n];
    let admit = |until: u64, events: &mut Vec<(u64,u8,usize)>,
                 queues: &mut Vec<VecDeque<usize>>, done: &mut Vec<u64>,
                 finished_count: &mut usize, levels: &mut Vec<usize>| {
        events.sort_unstable();
        while !events.is_empty() && events[0].0 <= until {
            let (at, kind, i) = events.remove(0);
            let _ = (at, &done, &finished_count);
            if kind == 1 { levels[i] = levels[i].saturating_sub(1); }
            queues[levels[i]].push_back(i);
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
                last = None;
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
        let burst_done = remaining[i] == 0;
        let finished = burst_done && positions[i] + 1 == jobs[i].bursts.len();
        if burst_done && !finished {
            let delay = jobs[i].bursts[positions[i] + 1];
            positions[i] += 2;
            remaining[i] = jobs[i].bursts[positions[i]];
            events.push((now + delay as u64, 1, i));
        }
        runs.push(Run { id: jobs[i].id, start, end: now, finished, level });
        last = Some(jobs[i].id);
        admit(now, &mut events, &mut queues, &mut done, &mut finished_count, &mut levels);
        if finished { done[i] = now; finished_count += 1; }
        else if !burst_done {
            levels[i] = (level + 1).min(quanta.len() - 1);
            queues[levels[i]].push_back(i);
        }
    }
    let stats: Vec<Stat> = jobs.iter().enumerate().map(|(i,j)| {
        let turnaround = done[i] - j.arrival as u64;
        let service: u64 = j.bursts.iter().map(|&v| v as u64).sum();
        Stat { id: j.id, completion: done[i], turnaround, waiting: turnaround - service }
    }).collect();
    let mean = |total: u64| if n == 0 { 0 } else { (total * 100) / n as u64 };
    let mean_waiting_x100 = mean(stats.iter().map(|s| s.waiting).sum());
    let mean_turnaround_x100 = mean(stats.iter().map(|s| s.turnaround).sum());
    Ok(Schedule { runs, idle, stats, mean_waiting_x100, mean_turnaround_x100, end: now })
}
