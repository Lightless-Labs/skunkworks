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

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
enum EventKind {
    Arrival,
    Wake,
}

#[derive(Clone, Copy, Debug)]
struct Event {
    time: u64,
    kind: EventKind,
    index: usize,
    level: usize,
}

#[derive(Clone, Debug)]
struct JobState {
    burst_position: usize,
    remaining: u32,
    level: usize,
    completion: u64,
}

fn rounded_mean_x100(total: u64, count: usize) -> u64 {
    if count == 0 {
        0
    } else {
        (total * 100 + count as u64 / 2) / count as u64
    }
}

fn push_idle(idle: &mut Vec<(u64, u64)>, start: u64, end: u64) {
    if start >= end {
        return;
    }
    if let Some(last) = idle.last_mut() {
        if last.1 == start {
            last.1 = end;
            return;
        }
    }
    idle.push((start, end));
}

pub fn schedule(
    jobs: &[Job],
    quanta: &[u32],
    switch_cost: u32,
) -> Result<Schedule, Error> {
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

    let mut events: Vec<Event> = jobs
        .iter()
        .enumerate()
        .map(|(index, job)| Event {
            time: job.arrival as u64,
            kind: EventKind::Arrival,
            index,
            level: job.level,
        })
        .collect();
    events.sort_by_key(|event| (event.time, event.kind, event.index));

    let mut event_cursor = 0usize;
    let mut queues: Vec<VecDeque<usize>> = (0..quanta.len()).map(|_| VecDeque::new()).collect();
    let mut states: Vec<JobState> = jobs
        .iter()
        .map(|job| JobState {
            burst_position: 0,
            remaining: job.bursts[0],
            level: job.level,
            completion: 0,
        })
        .collect();

    let mut time = 0u64;
    let mut runs = Vec::new();
    let mut idle = Vec::new();
    let mut last_dispatch_id: Option<u32> = None;
    let mut completed = 0usize;

    while completed < jobs.len() {
        while event_cursor < events.len() && events[event_cursor].time <= time {
            let event = events[event_cursor];
            states[event.index].level = event.level;
            queues[event.level].push_back(event.index);
            event_cursor += 1;
        }

        let selected = queues.iter().position(|queue| !queue.is_empty());
        let Some(level) = selected else {
            if event_cursor < events.len() {
                let next_time = events[event_cursor].time;
                if next_time > time {
                    push_idle(&mut idle, time, next_time);
                    time = next_time;
                    last_dispatch_id = None;
                }
                continue;
            }
            break;
        };

        let index = queues[level].pop_front().unwrap();
        let id = jobs[index].id;

        if last_dispatch_id.is_some_and(|previous| previous != id) {
            time += switch_cost as u64;
        }

        while event_cursor < events.len() && events[event_cursor].time <= time {
            let event = events[event_cursor];
            states[event.index].level = event.level;
            queues[event.level].push_back(event.index);
            event_cursor += 1;
        }

        let quantum = quanta[level];
        let consumed = states[index].remaining.min(quantum);
        let start = time;
        time += consumed as u64;
        let end = time;
        states[index].remaining -= consumed;

        let burst_finished = states[index].remaining == 0;
        let final_burst = burst_finished && states[index].burst_position + 1 == jobs[index].bursts.len();

        runs.push(Run {
            id,
            start,
            end,
            finished: final_burst,
            level,
        });

        if final_burst {
            states[index].completion = end;
            completed += 1;
        } else if burst_finished {
            let io_position = states[index].burst_position + 1;
            let next_cpu_position = states[index].burst_position + 2;
            let wake_time = end + jobs[index].bursts[io_position] as u64;
            states[index].burst_position = next_cpu_position;
            states[index].remaining = jobs[index].bursts[next_cpu_position];
            let wake_level = level.saturating_sub(1);
            events.push(Event {
                time: wake_time,
                kind: EventKind::Wake,
                index,
                level: wake_level,
            });
            events[event_cursor..].sort_by_key(|event| (event.time, event.kind, event.index));
        } else {
            states[index].level = (level + 1).min(quanta.len() - 1);
        }

        while event_cursor < events.len() && events[event_cursor].time <= end {
            let event = events[event_cursor];
            states[event.index].level = event.level;
            queues[event.level].push_back(event.index);
            event_cursor += 1;
        }

        if !final_burst {
            if burst_finished {
                // The wake event is admitted above, including a zero-delay wake.
            } else {
                queues[states[index].level].push_back(index);
            }
        }

        last_dispatch_id = Some(id);
    }

    let stats: Vec<Stat> = jobs
        .iter()
        .enumerate()
        .map(|(index, job)| {
            let completion = states[index].completion;
            let turnaround = completion - job.arrival as u64;
            let cpu: u64 = job
                .bursts
                .iter()
                .step_by(2)
                .map(|&length| length as u64)
                .sum();
            let io: u64 = job
                .bursts
                .iter()
                .skip(1)
                .step_by(2)
                .map(|&length| length as u64)
                .sum();
            Stat {
                id: job.id,
                completion,
                turnaround,
                waiting: turnaround - cpu - io,
            }
        })
        .collect();

    let total_waiting: u64 = stats.iter().map(|stat| stat.waiting).sum();
    let total_turnaround: u64 = stats.iter().map(|stat| stat.turnaround).sum();

    Ok(Schedule {
        runs,
        idle,
        stats,
        mean_waiting_x100: rounded_mean_x100(total_waiting, jobs.len()),
        mean_turnaround_x100: rounded_mean_x100(total_turnaround, jobs.len()),
        end: time,
    })
}

use std::collections::VecDeque;