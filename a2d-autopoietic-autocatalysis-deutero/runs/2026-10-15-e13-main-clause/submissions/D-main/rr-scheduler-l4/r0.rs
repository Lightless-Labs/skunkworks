use std::collections::VecDeque;

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

#[derive(Clone, Copy, Debug)]
struct Event {
    time: u64,
    kind: u8,
    index: usize,
}

#[derive(Clone, Debug)]
struct State {
    burst_position: usize,
    remaining: u64,
    level: usize,
    completion: Option<u64>,
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
        if jobs[..index].iter().any(|previous| previous.id == job.id) {
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

    let mut events = Vec::with_capacity(jobs.len() * 16);
    for (index, job) in jobs.iter().enumerate() {
        events.push(Event {
            time: u64::from(job.arrival),
            kind: 0,
            index,
        });
    }
    events.sort_by_key(|event| (event.time, event.kind, event.index));

    let mut queues: Vec<VecDeque<usize>> =
        (0..quanta.len()).map(|_| VecDeque::new()).collect();
    let mut states: Vec<State> = jobs
        .iter()
        .map(|job| State {
            burst_position: 0,
            remaining: u64::from(job.bursts[0]),
            level: job.level,
            completion: None,
        })
        .collect();

    let mut runs = Vec::new();
    let mut idle: Vec<(u64, u64)> = Vec::new();
    let mut time = 0u64;
    let mut event_cursor = 0usize;
    let mut last_dispatch_id: Option<u32> = None;
    let mut completed = 0usize;

    while completed < jobs.len() {
        while event_cursor < events.len() && events[event_cursor].time <= time {
            let event = events[event_cursor];
            let level = if event.kind == 0 {
                jobs[event.index].level
            } else {
                states[event.index].level
            };
            queues[level].push_back(event.index);
            event_cursor += 1;
        }

        let Some(level) = queues.iter().position(|queue| !queue.is_empty()) else {
            if event_cursor >= events.len() {
                break;
            }
            let next_time = events[event_cursor].time;
            if next_time > time {
                if let Some(last) = idle.last_mut() {
                    if last.1 == time {
                        last.1 = next_time;
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

        let index = queues[level].pop_front().unwrap();
        let job_id = jobs[index].id;
        let dispatch_level = states[index].level;

        if last_dispatch_id.is_some_and(|last_id| last_id != job_id) {
            time += u64::from(switch_cost);
        }

        while event_cursor < events.len() && events[event_cursor].time <= time {
            let event = events[event_cursor];
            let event_level = if event.kind == 0 {
                jobs[event.index].level
            } else {
                states[event.index].level
            };
            queues[event_level].push_back(event.index);
            event_cursor += 1;
        }

        let consumed = states[index]
            .remaining
            .min(u64::from(quanta[dispatch_level]));
        let start = time;
        time += consumed;
        let end = time;
        states[index].remaining -= consumed;

        let burst_finished = states[index].remaining == 0;
        let job_finished = burst_finished
            && states[index].burst_position + 1 == jobs[index].bursts.len();

        runs.push(Run {
            id: job_id,
            start,
            end,
            finished: job_finished,
            level: dispatch_level,
        });

        while event_cursor < events.len() && events[event_cursor].time <= end {
            let event = events[event_cursor];
            let event_level = if event.kind == 0 {
                jobs[event.index].level
            } else {
                states[event.index].level
            };
            queues[event_level].push_back(event.index);
            event_cursor += 1;
        }

        if job_finished {
            states[index].completion = Some(end);
            completed += 1;
        } else if !burst_finished {
            states[index].level = (dispatch_level + 1).min(quanta.len() - 1);
            queues[states[index].level].push_back(index);
        } else {
            let io_position = states[index].burst_position + 1;
            let io_delay = u64::from(jobs[index].bursts[io_position]);
            states[index].burst_position += 2;
            states[index].remaining =
                u64::from(jobs[index].bursts[states[index].burst_position]);
            states[index].level = dispatch_level.saturating_sub(1);
            events.push(Event {
                time: end + io_delay,
                kind: 1,
                index,
            });
            events[event_cursor..].sort_by_key(|event| (event.time, event.kind, event.index));
        }

        last_dispatch_id = Some(job_id);
    }

    let mut stats = Vec::with_capacity(jobs.len());
    let mut total_waiting = 0u64;
    let mut total_turnaround = 0u64;

    for (index, job) in jobs.iter().enumerate() {
        let completion = states[index].completion.unwrap_or(time);
        let turnaround = completion - u64::from(job.arrival);
        let work_and_io: u64 = job.bursts.iter().map(|&value| u64::from(value)).sum();
        let waiting = turnaround - work_and_io;

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
    let mean_waiting_x100 = if count == 0 {
        0
    } else {
        (total_waiting * 100 + count / 2) / count
    };
    let mean_turnaround_x100 = if count == 0 {
        0
    } else {
        (total_turnaround * 100 + count / 2) / count
    };

    Ok(Schedule {
        runs,
        idle,
        stats,
        mean_waiting_x100,
        mean_turnaround_x100,
        end: time,
    })
}

fn main() {}