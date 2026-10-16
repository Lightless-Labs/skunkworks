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
    burst_position: usize,
    remaining_cpu: u64,
    level: usize,
    completion: Option<u64>,
}

#[derive(Clone, Copy, Debug)]
struct Event {
    time: u64,
    kind: u8,
    job_index: usize,
    level: usize,
}

fn validate(jobs: &[Job], quanta: &[u32], switch_cost: u32) -> Result<(), Error> {
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

    Ok(())
}

fn mean_x100(total: u64, count: usize) -> u64 {
    if count == 0 {
        return 0;
    }
    let divisor = count as u64;
    let numerator = total * 100;
    (numerator + divisor / 2) / divisor
}

pub fn schedule(jobs: &[Job], quanta: &[u32], switch_cost: u32) -> Result<Schedule, Error> {
    validate(jobs, quanta, switch_cost)?;

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
            burst_position: 0,
            remaining_cpu: job.bursts[0] as u64,
            level: job.level,
            completion: None,
        })
        .collect();

    let mut events: Vec<Event> = jobs
        .iter()
        .enumerate()
        .map(|(job_index, job)| Event {
            time: job.arrival as u64,
            kind: 0,
            job_index,
            level: job.level,
        })
        .collect();

    let mut queues: Vec<VecDeque<usize>> =
        (0..quanta.len()).map(|_| VecDeque::new()).collect();

    let mut runs = Vec::new();
    let mut idle: Vec<(u64, u64)> = Vec::new();
    let mut time = 0_u64;
    let mut last_dispatch_id: Option<u32> = None;
    let mut completed_count = 0;

    fn admit_events(
        events: &mut Vec<Event>,
        queues: &mut [VecDeque<usize>],
        through: u64,
    ) {
        events.sort_by_key(|event| (event.time, event.kind, event.job_index));
        let count = events.partition_point(|event| event.time <= through);
        let admitted: Vec<Event> = events.drain(..count).collect();
        for event in admitted {
            queues[event.level].push_back(event.job_index);
        }
    }

    while completed_count < jobs.len() {
        admit_events(&mut events, &mut queues, time);

        let selected = queues.iter_mut().find_map(|queue| queue.pop_front());

        let Some(job_index) = selected else {
            if events.is_empty() {
                break;
            }

            events.sort_by_key(|event| (event.time, event.kind, event.job_index));
            let next_time = events[0].time;
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

        let level = processes[job_index].level;
        let id = jobs[job_index].id;

        if last_dispatch_id.is_some_and(|last_id| last_id != id) {
            time += switch_cost as u64;
        }
        admit_events(&mut events, &mut queues, time);

        let start = time;
        let quantum = quanta[level] as u64;
        let consumed = processes[job_index].remaining_cpu.min(quantum);
        time += consumed;
        let end = time;
        processes[job_index].remaining_cpu -= consumed;

        let burst_finished = processes[job_index].remaining_cpu == 0;
        let job_finished = burst_finished
            && processes[job_index].burst_position + 1 == jobs[job_index].bursts.len();

        runs.push(Run {
            id,
            start,
            end,
            finished: job_finished,
            level,
        });

        admit_events(&mut events, &mut queues, end);
        last_dispatch_id = Some(id);

        if burst_finished {
            if job_finished {
                processes[job_index].completion = Some(end);
                completed_count += 1;
            } else {
                let io_position = processes[job_index].burst_position + 1;
                let next_cpu_position = io_position + 1;
                let wake_time = end + jobs[job_index].bursts[io_position] as u64;
                let wake_level = level.saturating_sub(1);

                processes[job_index].burst_position = next_cpu_position;
                processes[job_index].remaining_cpu =
                    jobs[job_index].bursts[next_cpu_position] as u64;
                processes[job_index].level = wake_level;

                events.push(Event {
                    time: wake_time,
                    kind: 1,
                    job_index,
                    level: wake_level,
                });
                admit_events(&mut events, &mut queues, end);
            }
        } else {
            processes[job_index].level = (level + 1).min(quanta.len() - 1);
            queues[processes[job_index].level].push_back(job_index);
        }
    }

    let mut stats = Vec::with_capacity(jobs.len());
    let mut total_waiting = 0_u64;
    let mut total_turnaround = 0_u64;
    let mut schedule_end = 0_u64;

    for (index, job) in jobs.iter().enumerate() {
        let completion = processes[index].completion.unwrap_or(time);
        let turnaround = completion - job.arrival as u64;
        let cpu_total: u64 = job
            .bursts
            .iter()
            .step_by(2)
            .map(|&burst| burst as u64)
            .sum();
        let io_total: u64 = job
            .bursts
            .iter()
            .skip(1)
            .step_by(2)
            .map(|&burst| burst as u64)
            .sum();
        let waiting = turnaround - cpu_total - io_total;

        total_waiting += waiting;
        total_turnaround += turnaround;
        schedule_end = schedule_end.max(completion);

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
        mean_waiting_x100: mean_x100(total_waiting, jobs.len()),
        mean_turnaround_x100: mean_x100(total_turnaround, jobs.len()),
        end: schedule_end,
    })
}