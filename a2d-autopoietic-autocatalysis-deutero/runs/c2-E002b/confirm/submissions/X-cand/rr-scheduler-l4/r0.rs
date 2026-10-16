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

#[derive(Clone, Debug)]
struct Process {
    remaining: u32,
    burst_index: usize,
    level: usize,
    dispatch_level: usize,
    completion: Option<u64>,
}

#[derive(Clone, Copy, Debug)]
struct Event {
    time: u64,
    kind: u8,
    index: usize,
}

fn rounded_mean_x100(total: u64, count: usize) -> u64 {
    if count == 0 {
        return 0;
    }
    let numerator = u128::from(total) * 100;
    let denominator = count as u128;
    ((numerator * 2 + denominator) / (denominator * 2)) as u64
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

    let mut queues: Vec<VecDeque<usize>> =
        (0..quanta.len()).map(|_| VecDeque::new()).collect();
    let mut processes: Vec<Process> = jobs
        .iter()
        .map(|job| Process {
            remaining: job.bursts[0],
            burst_index: 0,
            level: job.level,
            dispatch_level: job.level,
            completion: None,
        })
        .collect();

    let mut events = Vec::new();
    for (index, job) in jobs.iter().enumerate() {
        events.push(Event {
            time: u64::from(job.arrival),
            kind: 0,
            index,
        });
    }
    events.sort_by_key(|event| (event.time, event.kind, event.index));

    let mut event_cursor = 0;
    let mut time = 0_u64;
    let mut last_dispatch: Option<u32> = None;
    let mut runs = Vec::new();
    let mut idle: Vec<(u64, u64)> = Vec::new();

    while processes.iter().any(|process| process.completion.is_none()) {
        while event_cursor < events.len() && events[event_cursor].time <= time {
            let event = events[event_cursor];
            if event.kind == 0 {
                queues[jobs[event.index].level].push_back(event.index);
            } else {
                let level = processes[event.index].level;
                queues[level].push_back(event.index);
            }
            event_cursor += 1;
        }

        let selected = queues.iter_mut().find_map(VecDeque::pop_front);
        let Some(index) = selected else {
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
                last_dispatch = None;
            }
            continue;
        };

        let id = jobs[index].id;
        if last_dispatch.is_some_and(|previous| previous != id) {
            time += u64::from(switch_cost);
        }

        while event_cursor < events.len() && events[event_cursor].time <= time {
            let event = events[event_cursor];
            if event.kind == 0 {
                queues[jobs[event.index].level].push_back(event.index);
            } else {
                let level = processes[event.index].level;
                queues[level].push_back(event.index);
            }
            event_cursor += 1;
        }

        let level = processes[index].level;
        processes[index].dispatch_level = level;
        let consumed = processes[index].remaining.min(quanta[level]);
        let start = time;
        time += u64::from(consumed);
        processes[index].remaining -= consumed;

        let burst_finished = processes[index].remaining == 0;
        let finished = burst_finished
            && processes[index].burst_index + 1 == jobs[index].bursts.len() - 1;

        runs.push(Run {
            id,
            start,
            end: time,
            finished,
            level,
        });

        while event_cursor < events.len() && events[event_cursor].time <= time {
            let event = events[event_cursor];
            if event.kind == 0 {
                queues[jobs[event.index].level].push_back(event.index);
            } else {
                let wake_level = processes[event.index].level;
                queues[wake_level].push_back(event.index);
            }
            event_cursor += 1;
        }

        if burst_finished {
            if finished {
                processes[index].completion = Some(time);
            } else {
                processes[index].burst_index += 2;
                processes[index].remaining = jobs[index].bursts[processes[index].burst_index];
                let io_delay =
                    u64::from(jobs[index].bursts[processes[index].burst_index - 1]);
                processes[index].level = processes[index].dispatch_level.saturating_sub(1);

                let wake = Event {
                    time: time + io_delay,
                    kind: 1,
                    index,
                };
                let insertion = events[event_cursor..].partition_point(|event| {
                    (event.time, event.kind, event.index)
                        <= (wake.time, wake.kind, wake.index)
                });
                events.insert(event_cursor + insertion, wake);
            }
        } else {
            processes[index].level = (level + 1).min(quanta.len() - 1);
            queues[processes[index].level].push_back(index);
        }

        last_dispatch = Some(id);
    }

    let mut stats = Vec::with_capacity(jobs.len());
    let mut total_waiting = 0_u64;
    let mut total_turnaround = 0_u64;
    let mut end = 0_u64;

    for (index, job) in jobs.iter().enumerate() {
        let completion = processes[index].completion.unwrap_or(0);
        let turnaround = completion - u64::from(job.arrival);
        let work: u64 = job.bursts.iter().map(|&burst| u64::from(burst)).sum();
        let waiting = turnaround - work;
        total_waiting += waiting;
        total_turnaround += turnaround;
        end = end.max(completion);
        stats.push(Stat {
            id: job.id,
            completion,
            turnaround,
            waiting,
        });
    }

    Ok(Schedule {
        runs,
        idle,
        stats,
        mean_waiting_x100: rounded_mean_x100(total_waiting, jobs.len()),
        mean_turnaround_x100: rounded_mean_x100(total_turnaround, jobs.len()),
        end,
    })
}