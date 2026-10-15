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
struct SimJob {
    id: u32,
    bursts: Vec<u32>,
    level: usize,
    burst_index: usize,
    remaining: u32,
}

#[derive(Clone, Copy, Debug)]
struct Event {
    time: u64,
    kind: u8,
    index: usize,
    level: usize,
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
        if job.bursts.is_empty()
            || job.bursts.len() > 31
            || job.bursts.len() % 2 == 0
        {
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

    let mut sim_jobs: Vec<SimJob> = jobs
        .iter()
        .map(|job| SimJob {
            id: job.id,
            bursts: job.bursts.clone(),
            level: job.level,
            burst_index: 0,
            remaining: job.bursts[0],
        })
        .collect();

    let mut events: Vec<Event> = jobs
        .iter()
        .enumerate()
        .map(|(index, job)| Event {
            time: u64::from(job.arrival),
            kind: 0,
            index,
            level: job.level,
        })
        .collect();
    events.sort_by_key(|event| (event.time, event.kind, event.index));

    let mut queues: Vec<std::collections::VecDeque<usize>> = (0..quanta.len())
        .map(|_| std::collections::VecDeque::new())
        .collect();
    let mut event_pos = 0usize;
    let mut time = 0u64;
    let mut last_id: Option<u32> = None;
    let mut runs = Vec::new();
    let mut idle: Vec<(u64, u64)> = Vec::new();
    let mut completions = vec![0u64; jobs.len()];
    let mut completed = 0usize;

    while completed < jobs.len() {
        while event_pos < events.len() && events[event_pos].time <= time {
            let event = events[event_pos];
            sim_jobs[event.index].level = event.level;
            queues[event.level].push_back(event.index);
            event_pos += 1;
        }

        let Some(level) = queues.iter().position(|queue| !queue.is_empty()) else {
            if event_pos >= events.len() {
                break;
            }

            let next_time = events[event_pos].time;
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
                last_id = None;
            }
            continue;
        };

        let index = queues[level].pop_front().unwrap();
        let selected_id = sim_jobs[index].id;

        if last_id.is_some_and(|previous| previous != selected_id) {
            time += u64::from(switch_cost);
        }

        while event_pos < events.len() && events[event_pos].time <= time {
            let event = events[event_pos];
            sim_jobs[event.index].level = event.level;
            queues[event.level].push_back(event.index);
            event_pos += 1;
        }

        let run_start = time;
        let consumed = sim_jobs[index].remaining.min(quanta[level]);
        time += u64::from(consumed);
        sim_jobs[index].remaining -= consumed;

        let burst_finished = sim_jobs[index].remaining == 0;
        let job_finished = burst_finished
            && sim_jobs[index].burst_index + 1 == sim_jobs[index].bursts.len();

        runs.push(Run {
            id: selected_id,
            start: run_start,
            end: time,
            finished: job_finished,
            level,
        });

        while event_pos < events.len() && events[event_pos].time <= time {
            let event = events[event_pos];
            sim_jobs[event.index].level = event.level;
            queues[event.level].push_back(event.index);
            event_pos += 1;
        }

        if job_finished {
            completions[index] = time;
            completed += 1;
        } else if burst_finished {
            let io_position = sim_jobs[index].burst_index + 1;
            let wake_time = time + u64::from(sim_jobs[index].bursts[io_position]);
            let next_cpu = io_position + 1;
            sim_jobs[index].burst_index = next_cpu;
            sim_jobs[index].remaining = sim_jobs[index].bursts[next_cpu];

            events.push(Event {
                time: wake_time,
                kind: 1,
                index,
                level: level.saturating_sub(1),
            });
            events[event_pos..].sort_by_key(|event| (event.time, event.kind, event.index));
        } else {
            sim_jobs[index].level = (level + 1).min(quanta.len() - 1);
            queues[sim_jobs[index].level].push_back(index);
        }

        last_id = Some(selected_id);
    }

    let mut stats = Vec::with_capacity(jobs.len());
    let mut total_waiting = 0u64;
    let mut total_turnaround = 0u64;

    for (index, job) in jobs.iter().enumerate() {
        let completion = completions[index];
        let turnaround = completion - u64::from(job.arrival);
        let cpu_and_io: u64 = job.bursts.iter().map(|&burst| u64::from(burst)).sum();
        let io: u64 = job
            .bursts
            .iter()
            .enumerate()
            .filter(|(position, _)| position % 2 == 1)
            .map(|(_, &burst)| u64::from(burst))
            .sum();
        let cpu: u64 = cpu_and_io - io;
        let waiting = turnaround - cpu - io;

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