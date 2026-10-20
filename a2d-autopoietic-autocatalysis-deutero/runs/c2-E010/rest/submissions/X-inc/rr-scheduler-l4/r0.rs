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
enum EventKind {
    Arrival,
    Wake { level: usize },
}

#[derive(Clone, Debug)]
struct Event {
    time: u64,
    kind_order: u8,
    index: usize,
    kind: EventKind,
}

#[derive(Clone, Debug)]
struct Process {
    remaining_cpu: u64,
    burst_index: usize,
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

fn rounded_mean_x100(total: u64, count: usize) -> u64 {
    if count == 0 {
        return 0;
    }
    let numerator = total * 100;
    let divisor = count as u64;
    (numerator + divisor / 2) / divisor
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

    let mut processes: Vec<Process> = jobs
        .iter()
        .map(|job| Process {
            remaining_cpu: u64::from(job.bursts[0]),
            burst_index: 0,
            level: job.level,
            completion: None,
        })
        .collect();

    let mut events: Vec<Event> = jobs
        .iter()
        .enumerate()
        .map(|(index, job)| Event {
            time: u64::from(job.arrival),
            kind_order: 0,
            index,
            kind: EventKind::Arrival,
        })
        .collect();
    events.sort_by_key(|event| (event.time, event.kind_order, event.index));

    let mut event_cursor = 0usize;
    let mut queues: Vec<std::collections::VecDeque<usize>> =
        (0..quanta.len()).map(|_| std::collections::VecDeque::new()).collect();

    let mut time = 0u64;
    let mut last_dispatched_id: Option<u32> = None;
    let mut runs = Vec::new();
    let mut idle = Vec::new();

    while processes.iter().any(|process| process.completion.is_none()) {
        while event_cursor < events.len() && events[event_cursor].time <= time {
            let event = events[event_cursor].clone();
            match event.kind {
                EventKind::Arrival => {
                    queues[jobs[event.index].level].push_back(event.index);
                }
                EventKind::Wake { level } => {
                    queues[level].push_back(event.index);
                }
            }
            event_cursor += 1;
        }

        let selected_level = queues.iter().position(|queue| !queue.is_empty());
        let Some(selected_level) = selected_level else {
            if event_cursor >= events.len() {
                break;
            }
            let next_time = events[event_cursor].time;
            if next_time > time {
                add_idle(&mut idle, time, next_time);
                time = next_time;
                last_dispatched_id = None;
            }
            continue;
        };

        let job_index = queues[selected_level].pop_front().unwrap();
        let job_id = jobs[job_index].id;

        if last_dispatched_id.is_some_and(|previous| previous != job_id) {
            time += u64::from(switch_cost);
        }

        while event_cursor < events.len() && events[event_cursor].time <= time {
            let event = events[event_cursor].clone();
            match event.kind {
                EventKind::Arrival => {
                    queues[jobs[event.index].level].push_back(event.index);
                }
                EventKind::Wake { level } => {
                    queues[level].push_back(event.index);
                }
            }
            event_cursor += 1;
        }

        let quantum = u64::from(quanta[selected_level]);
        let executed = processes[job_index].remaining_cpu.min(quantum);
        let run_start = time;
        time += executed;
        let run_end = time;

        processes[job_index].remaining_cpu -= executed;
        let burst_finished = processes[job_index].remaining_cpu == 0;
        let final_burst = processes[job_index].burst_index + 1 == jobs[job_index].bursts.len() / 2 + 1;
        let finished = burst_finished && final_burst;

        runs.push(Run {
            id: job_id,
            start: run_start,
            end: run_end,
            finished,
            level: selected_level,
        });

        while event_cursor < events.len() && events[event_cursor].time <= run_end {
            let event = events[event_cursor].clone();
            match event.kind {
                EventKind::Arrival => {
                    queues[jobs[event.index].level].push_back(event.index);
                }
                EventKind::Wake { level } => {
                    queues[level].push_back(event.index);
                }
            }
            event_cursor += 1;
        }

        if !burst_finished {
            processes[job_index].level = (selected_level + 1).min(quanta.len() - 1);
            queues[processes[job_index].level].push_back(job_index);
        } else if finished {
            processes[job_index].completion = Some(run_end);
        } else {
            let io_position = processes[job_index].burst_index + 1;
            let io_delay = u64::from(jobs[job_index].bursts[io_position]);
            let next_cpu_position = io_position + 1;
            processes[job_index].burst_index += 1;
            processes[job_index].remaining_cpu =
                u64::from(jobs[job_index].bursts[next_cpu_position]);

            let wake_level = selected_level.saturating_sub(1);
            let wake_event = Event {
                time: run_end + io_delay,
                kind_order: 1,
                index: job_index,
                kind: EventKind::Wake { level: wake_level },
            };
            let insert_at = events.partition_point(|event| {
                (event.time, event.kind_order, event.index)
                    <= (
                        wake_event.time,
                        wake_event.kind_order,
                        wake_event.index,
                    )
            });
            events.insert(insert_at, wake_event);
            if insert_at < event_cursor {
                event_cursor += 1;
            }
        }

        last_dispatched_id = Some(job_id);
    }

    let mut stats = Vec::with_capacity(jobs.len());
    for (index, job) in jobs.iter().enumerate() {
        let completion = processes[index].completion.unwrap_or(time);
        let turnaround = completion - u64::from(job.arrival);
        let total_cpu: u64 = job
            .bursts
            .iter()
            .step_by(2)
            .map(|&burst| u64::from(burst))
            .sum();
        let total_io: u64 = job
            .bursts
            .iter()
            .skip(1)
            .step_by(2)
            .map(|&burst| u64::from(burst))
            .sum();
        stats.push(Stat {
            id: job.id,
            completion,
            turnaround,
            waiting: turnaround - total_cpu - total_io,
        });
    }

    let total_waiting: u64 = stats.iter().map(|stat| stat.waiting).sum();
    let total_turnaround: u64 = stats.iter().map(|stat| stat.turnaround).sum();
    let end = stats.iter().map(|stat| stat.completion).max().unwrap_or(0);

    Ok(Schedule {
        runs,
        idle,
        stats,
        mean_waiting_x100: rounded_mean_x100(total_waiting, jobs.len()),
        mean_turnaround_x100: rounded_mean_x100(total_turnaround, jobs.len()),
        end,
    })
}