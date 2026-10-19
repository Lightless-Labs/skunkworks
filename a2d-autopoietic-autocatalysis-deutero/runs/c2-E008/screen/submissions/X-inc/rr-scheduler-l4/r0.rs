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
    job: usize,
    level: usize,
}

struct RuntimeJob {
    burst_position: usize,
    remaining: u32,
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
        if jobs[..index].iter().any(|prior| prior.id == job.id) {
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
    let mut runtime = Vec::with_capacity(jobs.len());
    for (index, job) in jobs.iter().enumerate() {
        events.push(Event {
            time: job.arrival as u64,
            kind: 0,
            index,
            job: index,
            level: job.level,
        });
        runtime.push(RuntimeJob {
            burst_position: 0,
            remaining: job.bursts[0],
            level: job.level,
            completion: None,
        });
    }
    events.sort_by_key(|event| (event.time, event.kind, event.index));

    let mut queues: Vec<std::collections::VecDeque<usize>> =
        (0..quanta.len()).map(|_| std::collections::VecDeque::new()).collect();
    let mut event_cursor = 0usize;
    let mut now = 0u64;
    let mut last_dispatch: Option<u32> = None;
    let mut runs = Vec::new();
    let mut idle = Vec::new();
    let mut completed_count = 0usize;

    while completed_count < jobs.len() {
        while event_cursor < events.len() && events[event_cursor].time <= now {
            let event = events[event_cursor];
            runtime[event.job].level = event.level;
            queues[event.level].push_back(event.job);
            event_cursor += 1;
        }

        let Some(level) = queues.iter().position(|queue| !queue.is_empty()) else {
            if event_cursor >= events.len() {
                break;
            }
            let next_time = events[event_cursor].time;
            if next_time > now {
                if let Some((_, end)) = idle.last_mut() {
                    if *end == now {
                        *end = next_time;
                    } else {
                        idle.push((now, next_time));
                    }
                } else {
                    idle.push((now, next_time));
                }
                now = next_time;
                last_dispatch = None;
            }
            continue;
        };

        let job_index = queues[level].pop_front().unwrap();
        let job_id = jobs[job_index].id;

        if last_dispatch.is_some_and(|previous| previous != job_id) {
            now += switch_cost as u64;
        }

        while event_cursor < events.len() && events[event_cursor].time <= now {
            let event = events[event_cursor];
            runtime[event.job].level = event.level;
            queues[event.level].push_back(event.job);
            event_cursor += 1;
        }

        let quantum = quanta[level];
        let consumed = runtime[job_index].remaining.min(quantum);
        let start = now;
        now += consumed as u64;
        let end = now;

        runtime[job_index].remaining -= consumed;
        let burst_finished = runtime[job_index].remaining == 0;
        let final_burst = runtime[job_index].burst_position + 1 == jobs[job_index].bursts.len();
        let finished = burst_finished && final_burst;

        runs.push(Run {
            id: job_id,
            start,
            end,
            finished,
            level,
        });

        if finished {
            runtime[job_index].completion = Some(end);
            completed_count += 1;
        }

        // Admit all events at the run endpoint before requeueing the running job.
        while event_cursor < events.len() && events[event_cursor].time <= end {
            let event = events[event_cursor];
            runtime[event.job].level = event.level;
            queues[event.level].push_back(event.job);
            event_cursor += 1;
        }

        if burst_finished && !finished {
            let io_position = runtime[job_index].burst_position + 1;
            let io_delay = jobs[job_index].bursts[io_position] as u64;
            runtime[job_index].burst_position += 2;
            runtime[job_index].remaining =
                jobs[job_index].bursts[runtime[job_index].burst_position];

            let wake_level = level.saturating_sub(1);
            let wake = Event {
                time: end + io_delay,
                kind: 1,
                index: job_index,
                job: job_index,
                level: wake_level,
            };

            // Insert while retaining the unprocessed prefix and global event order.
            events.push(wake);
            events[event_cursor..].sort_by_key(|event| (event.time, event.kind, event.index));

            while event_cursor < events.len() && events[event_cursor].time <= end {
                let event = events[event_cursor];
                runtime[event.job].level = event.level;
                queues[event.level].push_back(event.job);
                event_cursor += 1;
            }
        } else if !burst_finished && !finished {
            let new_level = (level + 1).min(quanta.len() - 1);
            runtime[job_index].level = new_level;
            queues[new_level].push_back(job_index);
        }

        last_dispatch = Some(job_id);
    }

    let mut stats = Vec::with_capacity(jobs.len());
    let mut total_waiting = 0u64;
    let mut total_turnaround = 0u64;

    for (index, job) in jobs.iter().enumerate() {
        let completion = runtime[index].completion.unwrap_or(now);
        let turnaround = completion - job.arrival as u64;
        let total_work: u64 = job.bursts.iter().map(|&value| value as u64).sum();
        let waiting = turnaround - total_work;
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
    let end = stats.iter().map(|stat| stat.completion).max().unwrap_or(0);

    Ok(Schedule {
        runs,
        idle,
        stats,
        mean_waiting_x100,
        mean_turnaround_x100,
        end,
    })
}