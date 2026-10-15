use std::collections::VecDeque;
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Job { pub id: u32, pub ticks: u32 }
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Run { pub id: u32, pub start: u64, pub end: u64, pub finished: bool }
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Schedule { pub runs: Vec<Run>, pub completion: Vec<(u32, u64)>, pub end: u64 }
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Error { Quantum, SwitchCost, TooManyJobs, DuplicateId { index: usize }, Ticks { index: usize } }


pub fn schedule(jobs: &[Job], quantum: u32, switch_cost: u32) -> Result<Schedule, Error> {
    if quantum == 0 || quantum > 1000 { return Err(Error::Quantum); }
    if switch_cost > 1000 { return Err(Error::SwitchCost); }
    if jobs.len() > 64 { return Err(Error::TooManyJobs); }
    for (i, job) in jobs.iter().enumerate() {
        if jobs[..i].iter().any(|j| j.id == job.id) { return Err(Error::DuplicateId { index: i }); }
        if job.ticks > 1000 { return Err(Error::Ticks { index: i }); }
    }
    let mut queue: VecDeque<usize> = (0..jobs.len()).collect();
    let mut remaining: Vec<u32> = jobs.iter().map(|j| j.ticks).collect();
    let mut done = vec![0; jobs.len()];
    let mut runs = Vec::new();
    let mut now = 0u64;
    let mut last = None;
    while let Some(i) = queue.pop_front() {
        if remaining[i] == 0 { done[i] = now; continue; }
        if last.is_some() && last != Some(jobs[i].id) { now += switch_cost as u64; }
        let start = now;
        let used = quantum.min(remaining[i]);
        now += used as u64;
        remaining[i] -= used;
        let finished = remaining[i] == 0;
        runs.push(Run { id: jobs[i].id, start, end: now, finished });
        last = Some(jobs[i].id);
        if finished { done[i] = now; } else { queue.push_back(i); }
    }
    let mut completion: Vec<_> = jobs.iter().enumerate().map(|(i,j)| (j.id, done[i])).collect();
    completion.sort_by_key(|x| x.1);
    Ok(Schedule { runs, completion, end: now })
}
