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

    let mut queues: Vec<std::collections::VecDeque<usize>> =
        (0..quanta.len()).map(|_| std::collections::VecDeque::new()).collect();
    let mut remaining: Vec<u32> = jobs.iter().map(|job| job.ticks).collect();
    let mut levels: Vec<usize> = jobs.iter().map(|job| job.level).collect();
    let mut completion: Vec<(u32, u64)> =
        jobs.iter().map(|job| (job.id, 0)).collect();
    let mut runs = Vec::new();
    let mut idle: Vec<(u64, u64)> = Vec::new();

    let mut arrival_cursor = 0;
    let mut completed_count = 0;
    let mut time = 0u64;
    let mut last_dispatched_id: Option<u32> = None;

    while completed_count < jobs.len() {
        while arrival_cursor < arrival_order.len()
            && u64::from(jobs[arrival_order[arrival_cursor]].arrival) <= time
        {
            let index = arrival_order[arrival_cursor];
            let job = &jobs[index];
            if job.ticks == 0 {
                completion[index] = (job.id, u64::from(job.arrival));
                completed_count += 1;
            } else {
                queues[job.level].push_back(index);
            }
            arrival_cursor += 1;
        }

        let Some(queue_level) = queues.iter().position(|queue| !queue.is_empty()) else {
            if arrival_cursor >= arrival_order.len() {
                break;
            }

            let next_time = u64::from(jobs[arrival_order[arrival_cursor]].arrival);
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
        let job_id = jobs[index].id;

        if last_dispatched_id.is_some_and(|last_id| last_id != job_id) {
            time += u64::from(switch_cost);
        }

        while arrival_cursor < arrival_order.len()
            && u64::from(jobs[arrival_order[arrival_cursor]].arrival) <= time
        {
            let arrival_index = arrival_order[arrival_cursor];
            let arrival_job = &jobs[arrival_index];
            if arrival_job.ticks == 0 {
                completion[arrival_index] =
                    (arrival_job.id, u64::from(arrival_job.arrival));
                completed_count += 1;
            } else {
                queues[arrival_job.level].push_back(arrival_index);
            }
            arrival_cursor += 1;
        }

        let run_level = levels[index];
        let consumed = remaining[index].min(quanta[run_level]);
        let start = time;
        time += u64::from(consumed);
        remaining[index] -= consumed;
        let finished = remaining[index] == 0;

        runs.push(Run {
            id: job_id,
            start,
            end: time,
            finished,
            level: run_level,
        });

        while arrival_cursor < arrival_order.len()
            && u64::from(jobs[arrival_order[arrival_cursor]].arrival) <= time
        {
            let arrival_index = arrival_order[arrival_cursor];
            let arrival_job = &jobs[arrival_index];
            if arrival_job.ticks == 0 {
                completion[arrival_index] =
                    (arrival_job.id, u64::from(arrival_job.arrival));
                completed_count += 1;
            } else {
                queues[arrival_job.level].push_back(arrival_index);
            }
            arrival_cursor += 1;
        }

        if finished {
            completion[index] = (job_id, time);
            completed_count += 1;
        } else {
            levels[index] = (run_level + 1).min(quanta.len() - 1);
            queues[levels[index]].push_back(index);
        }

        last_dispatched_id = Some(job_id);
    }

    let end = completion
        .iter()
        .map(|(_, completed_at)| *completed_at)
        .max()
        .unwrap_or(0);

    Ok(Schedule {
        runs,
        idle,
        completion,
        end,
    })
}

fn main() {}