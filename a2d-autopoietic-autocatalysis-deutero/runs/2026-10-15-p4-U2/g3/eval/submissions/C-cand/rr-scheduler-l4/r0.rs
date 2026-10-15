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
struct Event {
    time: u64,
    kind: u8,
    index: usize,
    level: usize,
}

#[derive(Clone, Debug)]
struct JobState {
    burst_position: usize,
    remaining: u32,
    level: usize,
    completion: Option<u64>,
}

fn add_idle(idle: &mut Vec<(u64, u64)>, start: u64, end: u64) {
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

    let mut states: Vec<JobState> = jobs
        .iter()
        .map(|job| JobState {
            burst_position: 0,
            remaining: job.bursts[0],
            level: job.level,
            completion: None,
        })
        .collect();

    let mut events: Vec<Event> = jobs
        .iter()
        .enumerate()
        .map(|(index, job)| Event {
            time: job.arrival as u64,
            kind: 0,
            index,
            level: job.level,
        })
        .collect();
    events.sort_by_key(|event| (event.time, event.kind, event.index));

    let mut next_event = 0;
    let mut queues: Vec<std::collections::VecDeque<usize>> =
        (0..quanta.len()).map(|_| std::collections::VecDeque::new()).collect();
    let mut now = 0u64;
    let mut last_id: Option<u32> = None;
    let mut runs = Vec::new();
    let mut idle = Vec::new();
    let mut completed = 0usize;

    while completed < jobs.len() {
        while next_event < events.len() && events[next_event].time <= now {
            let event = &events[next_event];
            states[event.index].level = event.level;
            queues[event.level].push_back(event.index);
            next_event += 1;
        }

        let selected_level = queues.iter().position(|queue| !queue.is_empty());
        let Some(level) = selected_level else {
            if next_event >= events.len() {
                break;
            }
            let next_time = events[next_event].time;
            if next_time > now {
                add_idle(&mut idle, now, next_time);
                now = next_time;
                last_id = None;
            }
            continue;
        };

        let index = queues[level].pop_front().expect("selected queue is nonempty");
        let id = jobs[index].id;

        if last_id.is_some_and(|previous| previous != id) {
            now += switch_cost as u64;
        }

        while next_event < events.len() && events[next_event].time <= now {
            let event = &events[next_event];
            states[event.index].level = event.level;
            queues[event.level].push_back(event.index);
            next_event += 1;
        }

        let dispatch_level = states[index].level;
        let start = now;
        let quantum = quanta[dispatch_level];
        let consumed = states[index].remaining.min(quantum);
        now += consumed as u64;
        states[index].remaining -= consumed;

        let burst_finished = states[index].remaining == 0;
        let job_finished = burst_finished && states[index].burst_position + 1 == jobs[index].bursts.len();
        runs.push(Run {
            id,
            start,
            end: now,
            finished: job_finished,
            level: dispatch_level,
        });

        if job_finished {
            states[index].completion = Some(now);
            completed += 1;
        } else if burst_finished {
            let io_position = states[index].burst_position + 1;
            let next_cpu_position = io_position + 1;
            let wake_time = now + jobs[index].bursts[io_position] as u64;
            states[index].burst_position = next_cpu_position;
            states[index].remaining = jobs[index].bursts[next_cpu_position];
            let wake_level = dispatch_level.saturating_sub(1);
            events.push(Event {
                time: wake_time,
                kind: 1,
                index,
                level: wake_level,
            });
        }

        events[next_event..].sort_by_key(|event| (event.time, event.kind, event.index));
        while next_event < events.len() && events[next_event].time <= now {
            let event = &events[next_event];
            states[event.index].level = event.level;
            queues[event.level].push_back(event.index);
            next_event += 1;
        }

        if !burst_finished {
            let demoted_level = (dispatch_level + 1).min(quanta.len() - 1);
            states[index].level = demoted_level;
            queues[demoted_level].push_back(index);
        }

        last_id = Some(id);
    }

    let stats: Vec<Stat> = jobs
        .iter()
        .enumerate()
        .map(|(index, job)| {
            let completion = states[index].completion.unwrap_or(now);
            let turnaround = completion - job.arrival as u64;
            let total_cpu: u64 = job
                .bursts
                .iter()
                .step_by(2)
                .map(|&burst| burst as u64)
                .sum();
            let total_io: u64 = job
                .bursts
                .iter()
                .skip(1)
                .step_by(2)
                .map(|&burst| burst as u64)
                .sum();
            Stat {
                id: job.id,
                completion,
                turnaround,
                waiting: turnaround - total_cpu - total_io,
            }
        })
        .collect();

    let count = jobs.len() as u64;
    let waiting_total: u64 = stats.iter().map(|stat| stat.waiting).sum();
    let turnaround_total: u64 = stats.iter().map(|stat| stat.turnaround).sum();

    Ok(Schedule {
        runs,
        idle,
        stats,
        mean_waiting_x100: (waiting_total * 100 + count / 2) / count,
        mean_turnaround_x100: (turnaround_total * 100 + count / 2) / count,
        end: now,
    })
}

fn main() {}