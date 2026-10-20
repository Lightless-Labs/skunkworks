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

struct RuntimeJob {
    burst_position: usize,
    remaining: u64,
    level: usize,
}

fn event_key(event: &Event) -> (u64, usize, usize) {
    let kind_order = match event.kind {
        EventKind::Arrival => 0,
        EventKind::Wake { .. } => 1,
    };
    (event.time, kind_order, event.index)
}

fn admit_through(
    events: &[Event],
    next_event: &mut usize,
    through: u64,
    runtime: &[RuntimeJob],
    queues: &mut [std::collections::VecDeque<usize>],
) {
    while *next_event < events.len() && events[*next_event].time <= through {
        let event = events[*next_event];
        match event.kind {
            EventKind::Arrival => {
                let level = runtime[event.index].level;
                queues[level].push_back(event.index);
            }
            EventKind::Wake { level } => {
                queues[level].push_back(event.index);
            }
        }
        *next_event += 1;
    }
}

fn insert_idle(idle: &mut Vec<(u64, u64)>, start: u64, end: u64) {
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

fn rounded_mean_x100(total: u64, count: usize) -> u64 {
    if count == 0 {
        return 0;
    }
    let denominator = count as u64;
    (total * 100 + denominator / 2) / denominator
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
    events.sort_by_key(event_key);

    let mut runtime: Vec<RuntimeJob> = jobs
        .iter()
        .map(|job| RuntimeJob {
            burst_position: 0,
            remaining: u64::from(job.bursts[0]),
            level: job.level,
        })
        .collect();

    let mut queues: Vec<std::collections::VecDeque<usize>> =
        (0..quanta.len()).map(|_| std::collections::VecDeque::new()).collect();
    let mut next_event = 0usize;
    let mut completed = vec![None; jobs.len()];
    let mut completion_count = 0usize;
    let mut runs = Vec::new();
    let mut idle = Vec::new();
    let mut now = 0u64;
    let mut last_dispatch_id: Option<u32> = None;

    while completion_count < jobs.len() {
        admit_through(&events, &mut next_event, now, &runtime, &mut queues);

        let selected_level = queues.iter().position(|queue| !queue.is_empty());
        let Some(selected_level) = selected_level else {
            if next_event >= events.len() {
                break;
            }
            let next_time = events[next_event].time;
            if next_time > now {
                insert_idle(&mut idle, now, next_time);
                now = next_time;
                last_dispatch_id = None;
            }
            continue;
        };

        let index = queues[selected_level].pop_front().unwrap();
        let id = jobs[index].id;
        if last_dispatch_id.is_some_and(|last_id| last_id != id) {
            now += u64::from(switch_cost);
        }

        admit_through(&events, &mut next_event, now, &runtime, &mut queues);

        let run_start = now;
        let quantum = u64::from(quanta[selected_level]);
        let consumed = runtime[index].remaining.min(quantum);
        now += consumed;
        runtime[index].remaining -= consumed;

        let burst_finished = runtime[index].remaining == 0;
        let job_finished =
            burst_finished && runtime[index].burst_position + 1 == jobs[index].bursts.len();

        runs.push(Run {
            id,
            start: run_start,
            end: now,
            finished: job_finished,
            level: selected_level,
        });

        let mut new_wake = None;
        if burst_finished && !job_finished {
            runtime[index].burst_position += 2;
            runtime[index].remaining =
                u64::from(jobs[index].bursts[runtime[index].burst_position]);
            let io_delay = u64::from(jobs[index].bursts[runtime[index].burst_position - 1]);
            let wake_time = now + io_delay;
            let wake_level = selected_level.saturating_sub(1);
            new_wake = Some(Event {
                time: wake_time,
                kind: EventKind::Wake { level: wake_level },
                index,
            });
        }

        if let Some(wake) = new_wake {
            let insertion_point = events[next_event..]
                .partition_point(|event| event_key(event) <= event_key(&wake));
            events.insert(next_event + insertion_point, wake);
        }

        admit_through(&events, &mut next_event, now, &runtime, &mut queues);

        if !burst_finished {
            runtime[index].level = (selected_level + 1).min(quanta.len() - 1);
            queues[runtime[index].level].push_back(index);
        } else if job_finished {
            completed[index] = Some(now);
            completion_count += 1;
        }

        last_dispatch_id = Some(id);
    }

    let stats: Vec<Stat> = jobs
        .iter()
        .enumerate()
        .map(|(index, job)| {
            let completion = completed[index].unwrap_or(now);
            let turnaround = completion - u64::from(job.arrival);
            let cpu: u64 = job
                .bursts
                .iter()
                .step_by(2)
                .map(|&burst| u64::from(burst))
                .sum();
            let io: u64 = job
                .bursts
                .iter()
                .skip(1)
                .step_by(2)
                .map(|&burst| u64::from(burst))
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
        end: now,
    })
}