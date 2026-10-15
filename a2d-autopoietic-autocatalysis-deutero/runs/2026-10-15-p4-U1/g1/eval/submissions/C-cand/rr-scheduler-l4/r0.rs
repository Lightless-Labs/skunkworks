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

#[derive(Clone, Copy)]
struct Event {
    time: u64,
    kind: u8,
    index: usize,
    wake_level: usize,
}

#[derive(Clone, Copy)]
struct ReadyJob {
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
            time: job.arrival as u64,
            kind: 0,
            index,
            wake_level: 0,
        });
    }
    events.sort_by_key(|event| (event.time, event.kind, event.index));

    let mut queues: Vec<VecDeque<ReadyJob>> =
        (0..quanta.len()).map(|_| VecDeque::new()).collect();
    let mut burst_position = vec![0usize; jobs.len()];
    let mut remaining_cpu: Vec<u32> = jobs.iter().map(|job| job.bursts[0]).collect();
    let mut completion = vec![0u64; jobs.len()];

    let mut runs = Vec::new();
    let mut idle: Vec<(u64, u64)> = Vec::new();
    let mut time = 0u64;
    let mut event_cursor = 0usize;
    let mut finished_count = 0usize;
    let mut last_dispatch_id: Option<u32> = None;

    while finished_count < jobs.len() {
        while event_cursor < events.len() && events[event_cursor].time <= time {
            let event = events[event_cursor];
            let level = if event.kind == 0 {
                jobs[event.index].level
            } else {
                event.wake_level
            };
            queues[level].push_back(ReadyJob { index: event.index });
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

        let selected = queues[level].pop_front().unwrap();
        let job_index = selected.index;
        let job = &jobs[job_index];

        if last_dispatch_id.is_some_and(|id| id != job.id) {
            time += switch_cost as u64;
        }

        while event_cursor < events.len() && events[event_cursor].time <= time {
            let event = events[event_cursor];
            let event_level = if event.kind == 0 {
                jobs[event.index].level
            } else {
                event.wake_level
            };
            queues[event_level].push_back(ReadyJob { index: event.index });
            event_cursor += 1;
        }

        let start = time;
        let consumed = remaining_cpu[job_index].min(quanta[level]);
        time += consumed as u64;
        remaining_cpu[job_index] -= consumed;

        let burst_ended = remaining_cpu[job_index] == 0;
        let final_burst = burst_ended && burst_position[job_index] + 1 == job.bursts.len();

        runs.push(Run {
            id: job.id,
            start,
            end: time,
            finished: final_burst,
            level,
        });

        if burst_ended && !final_burst {
            let wake_time = time + job.bursts[burst_position[job_index] + 1] as u64;
            events.push(Event {
                time: wake_time,
                kind: 1,
                index: job_index,
                wake_level: level.saturating_sub(1),
            });
            events[event_cursor..].sort_by_key(|event| (event.time, event.kind, event.index));
        }

        while event_cursor < events.len() && events[event_cursor].time <= time {
            let event = events[event_cursor];
            let event_level = if event.kind == 0 {
                jobs[event.index].level
            } else {
                event.wake_level
            };
            queues[event_level].push_back(ReadyJob { index: event.index });
            event_cursor += 1;
        }

        last_dispatch_id = Some(job.id);

        if !burst_ended {
            let demoted_level = (level + 1).min(quanta.len() - 1);
            queues[demoted_level].push_back(ReadyJob { index: job_index });
        } else if final_burst {
            completion[job_index] = time;
            finished_count += 1;
        } else {
            burst_position[job_index] += 2;
            remaining_cpu[job_index] = job.bursts[burst_position[job_index]];
        }
    }

    let mut stats = Vec::with_capacity(jobs.len());
    let mut total_waiting = 0u64;
    let mut total_turnaround = 0u64;

    for (index, job) in jobs.iter().enumerate() {
        let turnaround = completion[index] - job.arrival as u64;
        let cpu_total: u64 = job
            .bursts
            .iter()
            .step_by(2)
            .map(|&value| value as u64)
            .sum();
        let io_total: u64 = job
            .bursts
            .iter()
            .skip(1)
            .step_by(2)
            .map(|&value| value as u64)
            .sum();
        let waiting = turnaround - cpu_total - io_total;
        total_waiting += waiting;
        total_turnaround += turnaround;
        stats.push(Stat {
            id: job.id,
            completion: completion[index],
            turnaround,
            waiting,
        });
    }

    let count = jobs.len() as u64;
    let mean_waiting_x100 = (total_waiting * 100 + count / 2) / count;
    let mean_turnaround_x100 = (total_turnaround * 100 + count / 2) / count;

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