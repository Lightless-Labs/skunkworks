#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Job {
    pub id: u32,
    pub arrival: u32,
    pub bursts: Vec<u32>,
    pub level: usize,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Run {
    pub id: u32,
    pub start: u64,
    pub end: u64,
    pub finished: bool,
    pub level: usize,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Schedule {
    pub runs: Vec<Run>,
    pub idle: Vec<(u64, u64)>,
    pub stats: Vec<Stat>,
    pub mean_waiting_x100: u64,
    pub mean_turnaround_x100: u64,
    pub end: u64,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Error {
    LevelCount,
    Quantum { level: usize },
    SwitchCost,
    TooManyJobs,
    DuplicateId { index: usize },
    Arrival { index: usize },
    InitialLevel { index: usize },
    BurstCount { index: usize },
    Burst { index: usize, position: usize },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Stat {
    pub id: u32,
    pub completion: u64,
    pub turnaround: u64,
    pub waiting: u64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum EventKind {
    Arrival,
    Wake { level: usize },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Event {
    time: u64,
    kind: EventKind,
    index: usize,
}

pub fn schedule(jobs: &[Job], quanta: &[u32], switch_cost: u32) -> Result<Schedule, Error> {
    if !(1..=8).contains(&quanta.len()) {
        return Err(Error::LevelCount);
    }

    for (level, &quantum) in quanta.iter().enumerate() {
        if !(1..=1000).contains(&quantum) {
            return Err(Error::Quantum { level });
        }
    }

    if switch_cost > 1000 {
        return Err(Error::SwitchCost);
    }

    if jobs.len() > 64 {
        return Err(Error::TooManyJobs);
    }

    for (index, job) in jobs.iter().enumerate() {
        if jobs[..index].iter().any(|earlier| earlier.id == job.id) {
            return Err(Error::DuplicateId { index });
        }
        if job.arrival > 1000 {
            return Err(Error::Arrival { index });
        }
        if job.level >= quanta.len() {
            return Err(Error::InitialLevel { index });
        }
        if job.bursts.is_empty() || job.bursts.len() > 31 || job.bursts.len() % 2 == 0 {
            return Err(Error::BurstCount { index });
        }
        for (position, &burst) in job.bursts.iter().enumerate() {
            let valid = if position % 2 == 0 {
                (1..=1000).contains(&burst)
            } else {
                burst <= 1000
            };
            if !valid {
                return Err(Error::Burst { index, position });
            }
        }
    }

    if jobs.is_empty() {
        return Ok(Schedule {
            runs: Vec::new(),
            idle: Vec::new(),
            stats: Vec::new(),
            mean_waiting_x100: 0,
            mean_turnaround_x100: 0,
            end: 0,
        });
    }

    let mut events = Vec::with_capacity(jobs.len() * 16);
    for (index, job) in jobs.iter().enumerate() {
        events.push(Event {
            time: u64::from(job.arrival),
            kind: EventKind::Arrival,
            index,
        });
    }
    events.sort_by_key(|event| {
        let kind = match event.kind {
            EventKind::Arrival => 0usize,
            EventKind::Wake { .. } => 1usize,
        };
        (event.time, kind, event.index)
    });

    let mut queues: Vec<VecDeque<usize>> = (0..quanta.len()).map(|_| VecDeque::new()).collect();
    let mut burst_position = vec![0usize; jobs.len()];
    let mut remaining = jobs
        .iter()
        .map(|job| u64::from(job.bursts[0]))
        .collect::<Vec<_>>();
    let mut completions = vec![0u64; jobs.len()];
    let mut runs = Vec::new();
    let mut idle = Vec::new();
    let mut time = 0u64;
    let mut event_cursor = 0usize;
    let mut last_dispatch_id: Option<u32> = None;
    let mut completed_jobs = 0usize;

    let admit_through = |limit: u64,
                         event_cursor: &mut usize,
                         events: &[Event],
                         queues: &mut Vec<VecDeque<usize>>| {
        while *event_cursor < events.len() && events[*event_cursor].time <= limit {
            let event = events[*event_cursor];
            let level = match event.kind {
                EventKind::Arrival => jobs[event.index].level,
                EventKind::Wake { level } => level,
            };
            queues[level].push_back(event.index);
            *event_cursor += 1;
        }
    };

    while completed_jobs < jobs.len() {
        admit_through(time, &mut event_cursor, &events, &mut queues);

        let selected_level = queues.iter().position(|queue| !queue.is_empty());
        let Some(level) = selected_level else {
            if event_cursor >= events.len() {
                break;
            }
            let next_time = events[event_cursor].time;
            if next_time > time {
                if let Some((_, end)) = idle.last_mut() {
                    if *end == time {
                        *end = next_time;
                    } else {
                        idle.push((time, next_time));
                    }
                } else {
                    idle.push((time, next_time));
                }
                time = next_time;
                last_dispatch_id = None;
            }
            continue;
        };

        let job_index = queues[level].pop_front().unwrap();
        let job_id = jobs[job_index].id;

        if last_dispatch_id.is_some_and(|previous| previous != job_id) {
            time += u64::from(switch_cost);
        }
        admit_through(time, &mut event_cursor, &events, &mut queues);

        let quantum = u64::from(quanta[level]);
        let consumed = remaining[job_index].min(quantum);
        let start = time;
        time += consumed;
        let end = time;
        remaining[job_index] -= consumed;

        let burst_finished = remaining[job_index] == 0;
        let job_finished = burst_finished && burst_position[job_index] + 1 == jobs[job_index].bursts.len();

        runs.push(Run {
            id: job_id,
            start,
            end,
            finished: job_finished,
            level,
        });

        if job_finished {
            completions[job_index] = end;
            completed_jobs += 1;
        } else if burst_finished {
            let io_position = burst_position[job_index] + 1;
            let wake_time = end + u64::from(jobs[job_index].bursts[io_position]);
            burst_position[job_index] += 2;
            remaining[job_index] = u64::from(jobs[job_index].bursts[burst_position[job_index]]);
            let wake_level = level.saturating_sub(1);
            events.push(Event {
                time: wake_time,
                kind: EventKind::Wake { level: wake_level },
                index: job_index,
            });
            events[event_cursor..].sort_by_key(|event| {
                let kind = match event.kind {
                    EventKind::Arrival => 0usize,
                    EventKind::Wake { .. } => 1usize,
                };
                (event.time, kind, event.index)
            });
        }

        admit_through(end, &mut event_cursor, &events, &mut queues);

        if !job_finished && !burst_finished {
            let demoted = (level + 1).min(quanta.len() - 1);
            queues[demoted].push_back(job_index);
        }

        last_dispatch_id = Some(job_id);
    }

    let mut stats = Vec::with_capacity(jobs.len());
    let mut total_waiting = 0u64;
    let mut total_turnaround = 0u64;

    for (index, job) in jobs.iter().enumerate() {
        let completion = completions[index];
        let turnaround = completion - u64::from(job.arrival);
        let cpu_time: u64 = job
            .bursts
            .iter()
            .step_by(2)
            .map(|&value| u64::from(value))
            .sum();
        let io_time: u64 = job
            .bursts
            .iter()
            .skip(1)
            .step_by(2)
            .map(|&value| u64::from(value))
            .sum();
        let waiting = turnaround - cpu_time - io_time;
        total_waiting += waiting;
        total_turnaround += turnaround;
        stats.push(Stat {
            id: job.id,
            completion,
            turnaround,
            waiting,
        });
    }

    let count = jobs.len() as u64;
    let mean_waiting_x100 = (total_waiting * 100 + count / 2) / count;
    let mean_turnaround_x100 = (total_turnaround * 100 + count / 2) / count;
    let end = completions.iter().copied().max().unwrap_or(0);

    Ok(Schedule {
        runs,
        idle,
        stats,
        mean_waiting_x100,
        mean_turnaround_x100,
        end,
    })
}

use std::collections::VecDeque;

fn main() {}