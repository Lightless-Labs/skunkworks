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
    id: u32,
    arrival: u64,
    bursts: Vec<u32>,
    level: usize,
    burst_pos: usize,
    remaining: u32,
    completion: Option<u64>,
}

#[derive(Clone, Copy, Debug)]
struct Event {
    time: u64,
    kind: u8,
    index: usize,
    level: usize,
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

    let count = jobs.len();
    let mut processes: Vec<Process> = jobs
        .iter()
        .map(|job| Process {
            id: job.id,
            arrival: job.arrival as u64,
            bursts: job.bursts.clone(),
            level: job.level,
            burst_pos: 0,
            remaining: job.bursts[0],
            completion: None,
        })
        .collect();

    let mut queues: Vec<std::collections::VecDeque<usize>> =
        (0..quanta.len()).map(|_| std::collections::VecDeque::new()).collect();

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

    let mut runs = Vec::new();
    let mut idle = Vec::new();
    let mut now = 0u64;
    let mut last_id: Option<u32> = None;
    let mut completed = 0usize;

    while completed < count {
        events.sort_by_key(|event| (event.time, event.kind, event.index));

        let mut event_index = 0;
        while event_index < events.len() && events[event_index].time <= now {
            let event = events.remove(event_index);
            processes[event.index].level = event.level;
            queues[event.level].push_back(event.index);
        }

        let selected_level = queues.iter().position(|queue| !queue.is_empty());

        if let Some(level) = selected_level {
            let index = queues[level].pop_front().unwrap();
            let id = processes[index].id;

            if last_id.is_some_and(|previous| previous != id) {
                now += switch_cost as u64;
            }

            events.sort_by_key(|event| (event.time, event.kind, event.index));
            let mut event_index = 0;
            while event_index < events.len() && events[event_index].time <= now {
                let event = events.remove(event_index);
                processes[event.index].level = event.level;
                queues[event.level].push_back(event.index);
            }

            let quantum = quanta[level];
            let consumed = processes[index].remaining.min(quantum);
            let start = now;
            now += consumed as u64;
            let end = now;
            processes[index].remaining -= consumed;

            let burst_finished = processes[index].remaining == 0;
            let final_burst = burst_finished
                && processes[index].burst_pos + 1 == processes[index].bursts.len();

            runs.push(Run {
                id,
                start,
                end,
                finished: final_burst,
                level,
            });

            if final_burst {
                processes[index].completion = Some(end);
                completed += 1;
            } else if burst_finished {
                let io_delay = processes[index].bursts[processes[index].burst_pos + 1] as u64;
                processes[index].burst_pos += 2;
                processes[index].remaining = processes[index].bursts[processes[index].burst_pos];
                events.push(Event {
                    time: end + io_delay,
                    kind: 1,
                    index,
                    level: level.saturating_sub(1),
                });
            }

            events.sort_by_key(|event| (event.time, event.kind, event.index));
            let mut event_index = 0;
            while event_index < events.len() && events[event_index].time <= end {
                let event = events.remove(event_index);
                processes[event.index].level = event.level;
                queues[event.level].push_back(event.index);
            }

            if !final_burst {
                if burst_finished {
                    // The process is blocked until its wake event.
                } else {
                    let next_level = level.saturating_add(1).min(quanta.len() - 1);
                    processes[index].level = next_level;
                    queues[next_level].push_back(index);
                }
            }

            last_id = Some(id);
        } else if !events.is_empty() {
            events.sort_by_key(|event| (event.time, event.kind, event.index));
            let next_time = events[0].time;
            if next_time > now {
                add_idle(&mut idle, now, next_time);
                now = next_time;
                last_id = None;
            }
        }
    }

    let mut stats = Vec::with_capacity(count);
    let mut waiting_total = 0u64;
    let mut turnaround_total = 0u64;

    for (index, process) in processes.iter().enumerate() {
        let completion = process.completion.unwrap_or(0);
        let turnaround = completion - process.arrival;
        let cpu_total: u64 = process
            .bursts
            .iter()
            .step_by(2)
            .map(|&value| value as u64)
            .sum();
        let io_total: u64 = process
            .bursts
            .iter()
            .skip(1)
            .step_by(2)
            .map(|&value| value as u64)
            .sum();
        let waiting = turnaround - cpu_total - io_total;

        stats.push(Stat {
            id: jobs[index].id,
            completion,
            turnaround,
            waiting,
        });
        waiting_total += waiting;
        turnaround_total += turnaround;
    }

    let mean_waiting_x100 = if count == 0 {
        0
    } else {
        (waiting_total * 100 + count as u64 / 2) / count as u64
    };
    let mean_turnaround_x100 = if count == 0 {
        0
    } else {
        (turnaround_total * 100 + count as u64 / 2) / count as u64
    };

    Ok(Schedule {
        end: now,
        runs,
        idle,
        stats,
        mean_waiting_x100,
        mean_turnaround_x100,
    })
}