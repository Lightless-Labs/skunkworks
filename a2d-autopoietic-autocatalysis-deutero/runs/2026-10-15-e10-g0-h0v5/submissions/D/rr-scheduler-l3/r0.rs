#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Job {
    pub id: u32,
    pub arrival: u32,
    pub ticks: u32,
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
    pub completion: Vec<(u32, u64)>,
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
    Ticks { index: usize },
}

pub fn schedule(jobs: &[Job], quanta: &[u32], switch_cost: u32) -> Result<Schedule, Error> {
    use std::collections::VecDeque;

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
        if job.ticks > 1000 {
            return Err(Error::Ticks { index });
        }
    }

    let mut arrival_order: Vec<usize> = (0..jobs.len()).collect();
    arrival_order.sort_by_key(|&index| (jobs[index].arrival, index));

    let mut queues: Vec<VecDeque<usize>> =
        (0..quanta.len()).map(|_| VecDeque::new()).collect();
    let mut remaining: Vec<u32> = jobs.iter().map(|job| job.ticks).collect();
    let mut levels: Vec<usize> = jobs.iter().map(|job| job.level).collect();
    let mut completion: Vec<Option<u64>> = vec![None; jobs.len()];
    let mut runs: Vec<Run> = Vec::new();
    let mut idle: Vec<(u64, u64)> = Vec::new();

    let mut time = 0u64;
    let mut next_arrival = 0usize;
    let mut last_dispatched_id: Option<u32> = None;

    let admit_through = |time: u64,
                         next_arrival: &mut usize,
                         queues: &mut Vec<VecDeque<usize>>,
                         completion: &mut Vec<Option<u64>>| {
        while *next_arrival < arrival_order.len() {
            let index = arrival_order[*next_arrival];
            let job = &jobs[index];
            if u64::from(job.arrival) > time {
                break;
            }
            *next_arrival += 1;
            if job.ticks == 0 {
                completion[index] = Some(u64::from(job.arrival));
            } else {
                queues[job.level].push_back(index);
            }
        }
    };

    loop {
        admit_through(time, &mut next_arrival, &mut queues, &mut completion);

        let Some(queue_level) = queues.iter().position(|queue| !queue.is_empty()) else {
            if next_arrival == arrival_order.len() {
                break;
            }

            let next_time = u64::from(jobs[arrival_order[next_arrival]].arrival);
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
                last_dispatched_id = None;
            }
            continue;
        };

        let index = queues[queue_level].pop_front().unwrap();
        let job = &jobs[index];

        if let Some(last_id) = last_dispatched_id {
            if last_id != job.id {
                time += u64::from(switch_cost);
            }
        }

        admit_through(time, &mut next_arrival, &mut queues, &mut completion);

        let level = levels[index];
        let consumed = remaining[index].min(quanta[level]);
        let start = time;
        time += u64::from(consumed);
        remaining[index] -= consumed;
        let finished = remaining[index] == 0;

        runs.push(Run {
            id: job.id,
            start,
            end: time,
            finished,
            level,
        });

        admit_through(time, &mut next_arrival, &mut queues, &mut completion);

        if finished {
            completion[index] = Some(time);
        } else {
            levels[index] = (level + 1).min(quanta.len() - 1);
            queues[levels[index]].push_back(index);
        }

        last_dispatched_id = Some(job.id);
    }

    let completion = jobs
        .iter()
        .enumerate()
        .map(|(index, job)| (job.id, completion[index].unwrap()))
        .collect();

    Ok(Schedule {
        runs,
        idle,
        completion,
        end: time,
    })
}