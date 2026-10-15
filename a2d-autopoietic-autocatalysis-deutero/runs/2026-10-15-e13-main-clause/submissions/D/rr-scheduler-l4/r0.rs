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
struct SimJob {
    id: u32,
    bursts: Vec<u32>,
    burst_pos: usize,
    remaining: u32,
    level: usize,
    completion: Option<u64>,
}

#[derive(Clone, Copy, Debug)]
enum EventKind {
    Arrival,
    Wake { job: usize, level: usize },
}

#[derive(Clone, Copy, Debug)]
struct Event {
    time: u64,
    kind_order: u8,
    index: usize,
    kind: EventKind,
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

    let mut sim_jobs: Vec<SimJob> = jobs
        .iter()
        .map(|job| SimJob {
            id: job.id,
            bursts: job.bursts.clone(),
            burst_pos: 0,
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
            kind_order: 0,
            index,
            kind: EventKind::Arrival,
        })
        .collect();

    let mut queues: Vec<VecDeque<usize>> =
        (0..quanta.len()).map(|_| VecDeque::new()).collect();
    let mut runs = Vec::new();
    let mut idle: Vec<(u64, u64)> = Vec::new();
    let mut time = 0u64;
    let mut last_dispatch_id: Option<u32> = None;
    let mut completed_count = 0usize;

    while completed_count < jobs.len() {
        admit_events(&mut events, &mut sim_jobs, &mut queues, time);

        let selected = queues.iter_mut().find_map(VecDeque::pop_front);

        let Some(job_index) = selected else {
            let next_time = events
                .iter()
                .map(|event| event.time)
                .min()
                .expect("unfinished jobs have pending events");
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

        let level = sim_jobs[job_index].level;
        let id = sim_jobs[job_index].id;

        if last_dispatch_id.is_some_and(|last_id| last_id != id) {
            time += switch_cost as u64;
        }
        admit_events(&mut events, &mut sim_jobs, &mut queues, time);

        let start = time;
        let consumed = sim_jobs[job_index].remaining.min(quanta[level]);
        time += consumed as u64;
        sim_jobs[job_index].remaining -= consumed;

        let burst_done = sim_jobs[job_index].remaining == 0;
        let finished = burst_done
            && sim_jobs[job_index].burst_pos + 1 == sim_jobs[job_index].bursts.len();

        runs.push(Run {
            id,
            start,
            end: time,
            finished,
            level,
        });

        admit_events(&mut events, &mut sim_jobs, &mut queues, time);

        if finished {
            sim_jobs[job_index].completion = Some(time);
            completed_count += 1;
        } else if burst_done {
            let io_pos = sim_jobs[job_index].burst_pos + 1;
            let io_delay = sim_jobs[job_index].bursts[io_pos] as u64;
            sim_jobs[job_index].burst_pos += 2;
            sim_jobs[job_index].remaining = sim_jobs[job_index].bursts
                [sim_jobs[job_index].burst_pos];
            let wake_level = level.saturating_sub(1);
            events.push(Event {
                time: time + io_delay,
                kind_order: 1,
                index: job_index,
                kind: EventKind::Wake {
                    job: job_index,
                    level: wake_level,
                },
            });
            admit_events(&mut events, &mut sim_jobs, &mut queues, time);
        } else {
            sim_jobs[job_index].level = (level + 1).min(quanta.len() - 1);
            queues[sim_jobs[job_index].level].push_back(job_index);
        }

        last_dispatch_id = Some(id);
    }

    let stats: Vec<Stat> = jobs
        .iter()
        .enumerate()
        .map(|(index, job)| {
            let completion = sim_jobs[index]
                .completion
                .expect("all jobs must have completed");
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
                .map(|&delay| delay as u64)
                .sum();

            Stat {
                id: job.id,
                completion,
                turnaround,
                waiting: turnaround - cpu_total - io_total,
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
        end: time,
    })
}

fn admit_events(
    events: &mut Vec<Event>,
    jobs: &mut [SimJob],
    queues: &mut [VecDeque<usize>],
    through_time: u64,
) {
    events.sort_by_key(|event| (event.time, event.kind_order, event.index));
    let admitted_count = events.partition_point(|event| event.time <= through_time);
    let admitted: Vec<Event> = events.drain(..admitted_count).collect();

    for event in admitted {
        match event.kind {
            EventKind::Arrival => {
                let job_index = event.index;
                let level = jobs[job_index].level;
                queues[level].push_back(job_index);
            }
            EventKind::Wake { job, level } => {
                jobs[job].level = level;
                queues[level].push_back(job);
            }
        }
    }
}