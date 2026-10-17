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
    job: Job,
    burst_index: usize,
    remaining: u32,
    level: usize,
    completion: Option<u64>,
}

#[derive(Clone, Copy, Debug)]
struct Event {
    time: u64,
    kind: u8,
    index: usize,
    level: usize,
}

fn mean_x100(total: u64, count: usize) -> u64 {
    if count == 0 {
        0
    } else {
        let denominator = count as u64;
        (total * 100 + denominator / 2) / denominator
    }
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

    let mut processes: Vec<Process> = jobs
        .iter()
        .cloned()
        .map(|job| Process {
            remaining: job.bursts[0],
            level: job.level,
            job,
            burst_index: 0,
            completion: None,
        })
        .collect();

    let mut events: Vec<Event> = jobs
        .iter()
        .enumerate()
        .map(|(index, job)| Event {
            time: u64::from(job.arrival),
            kind: 0,
            index,
            level: 0,
        })
        .collect();

    let mut queues: Vec<std::collections::VecDeque<usize>> = (0..quanta.len())
        .map(|_| std::collections::VecDeque::new())
        .collect();

    let mut runs = Vec::new();
    let mut idle: Vec<(u64, u64)> = Vec::new();
    let mut now = 0u64;
    let mut last_dispatch_id: Option<u32> = None;
    let mut completed = 0usize;

    fn admit_through(
        events: &mut Vec<Event>,
        queues: &mut [std::collections::VecDeque<usize>],
        processes: &[Process],
        through: u64,
    ) {
        events.sort_by_key(|event| (event.time, event.kind, event.index));
        let count = events.partition_point(|event| event.time <= through);
        let admitted: Vec<Event> = events.drain(..count).collect();

        for event in admitted {
            match event.kind {
                0 => queues[processes[event.index].job.level].push_back(event.index),
                _ => {
                    let level = event.level.saturating_sub(1);
                    queues[level].push_back(event.index);
                }
            }
        }
    }

    while completed < jobs.len() {
        admit_through(&mut events, &mut queues, &processes, now);

        let selected_level = queues.iter().position(|queue| !queue.is_empty());
        let Some(level) = selected_level else {
            if events.is_empty() {
                break;
            }

            events.sort_by_key(|event| (event.time, event.kind, event.index));
            let next_time = events[0].time;

            if next_time > now {
                if let Some(last) = idle.last_mut() {
                    if last.1 == now {
                        last.1 = next_time;
                    } else {
                        idle.push((now, next_time));
                    }
                } else {
                    idle.push((now, next_time));
                }

                now = next_time;
                last_dispatch_id = None;
            }

            continue;
        };

        let process_index = queues[level].pop_front().unwrap();
        let process_id = processes[process_index].job.id;

        if last_dispatch_id.is_some_and(|last_id| last_id != process_id) {
            now += u64::from(switch_cost);
            admit_through(&mut events, &mut queues, &processes, now);
        }

        let quantum = quanta[level];
        let consumed = processes[process_index].remaining.min(quantum);
        let start = now;
        now += u64::from(consumed);
        let end = now;

        processes[process_index].remaining -= consumed;
        let burst_finished = processes[process_index].remaining == 0;
        let job_finished = burst_finished
            && processes[process_index].burst_index + 1 == processes[process_index].job.bursts.len();

        runs.push(Run {
            id: process_id,
            start,
            end,
            finished: job_finished,
            level,
        });

        if burst_finished && !job_finished {
            let io_position = processes[process_index].burst_index + 1;
            let delay = processes[process_index].job.bursts[io_position];
            let next_cpu_position = io_position + 1;
            let next_cpu = processes[process_index].job.bursts[next_cpu_position];

            processes[process_index].burst_index = next_cpu_position;
            processes[process_index].remaining = next_cpu;

            events.push(Event {
                time: end + u64::from(delay),
                kind: 1,
                index: process_index,
                level,
            });
        } else if job_finished {
            processes[process_index].completion = Some(end);
            completed += 1;
        }

        admit_through(&mut events, &mut queues, &processes, end);

        if !burst_finished {
            processes[process_index].level = (level + 1).min(quanta.len() - 1);
            queues[processes[process_index].level].push_back(process_index);
        }

        last_dispatch_id = Some(process_id);
    }

    let mut stats = Vec::with_capacity(jobs.len());
    let mut waiting_total = 0u64;
    let mut turnaround_total = 0u64;

    for process in &processes {
        let completion = process.completion.unwrap_or(now);
        let turnaround = completion - u64::from(process.job.arrival);
        let total_cpu: u64 = process
            .job
            .bursts
            .iter()
            .step_by(2)
            .map(|&burst| u64::from(burst))
            .sum();
        let total_io: u64 = process
            .job
            .bursts
            .iter()
            .skip(1)
            .step_by(2)
            .map(|&burst| u64::from(burst))
            .sum();
        let waiting = turnaround - total_cpu - total_io;

        waiting_total += waiting;
        turnaround_total += turnaround;
        stats.push(Stat {
            id: process.job.id,
            completion,
            turnaround,
            waiting,
        });
    }

    Ok(Schedule {
        runs,
        idle,
        stats,
        mean_waiting_x100: mean_x100(waiting_total, jobs.len()),
        mean_turnaround_x100: mean_x100(turnaround_total, jobs.len()),
        end: now,
    })
}