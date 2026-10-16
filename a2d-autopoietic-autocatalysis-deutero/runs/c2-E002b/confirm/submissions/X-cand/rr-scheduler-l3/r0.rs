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

    let mut order: Vec<usize> = (0..jobs.len()).collect();
    order.sort_by_key(|&index| (jobs[index].arrival, index));

    let mut queues: Vec<std::collections::VecDeque<usize>> =
        (0..quanta.len()).map(|_| std::collections::VecDeque::new()).collect();
    let mut remaining: Vec<u32> = jobs.iter().map(|job| job.ticks).collect();
    let mut levels: Vec<usize> = jobs.iter().map(|job| job.level).collect();
    let mut completion: Vec<Option<u64>> = vec![None; jobs.len()];
    let mut runs = Vec::new();
    let mut idle = Vec::new();
    let mut now = 0u64;
    let mut next_event = 0usize;
    let mut last_dispatched_id: Option<u32> = None;
    let mut completed_count = 0usize;

    while completed_count < jobs.len() {
        while next_event < order.len()
            && u64::from(jobs[order[next_event]].arrival) <= now
        {
            let index = order[next_event];
            let job = &jobs[index];
            let arrival = u64::from(job.arrival);

            if job.ticks == 0 {
                completion[index] = Some(arrival);
                completed_count += 1;
            } else {
                queues[job.level].push_back(index);
            }
            next_event += 1;
        }

        let selected_level = queues.iter().position(|queue| !queue.is_empty());

        if selected_level.is_none() {
            if next_event >= order.len() {
                break;
            }

            let next_time = u64::from(jobs[order[next_event]].arrival);
            if next_time > now {
                if let Some((_, end)) = idle.last_mut() {
                    if *end == now {
                        *end = next_time;
                    } else {
                        idle.push((now, next_time));
                    }
                } else {
                    idle.push((now, next_time));
                }
                now = next_time;
                last_dispatched_id = None;
            }
            continue;
        }

        let level = selected_level.unwrap();
        let index = queues[level].pop_front().unwrap();
        let job_id = jobs[index].id;

        if last_dispatched_id.is_some_and(|previous| previous != job_id) {
            now += u64::from(switch_cost);
        }

        while next_event < order.len()
            && u64::from(jobs[order[next_event]].arrival) <= now
        {
            let event_index = order[next_event];
            let event_job = &jobs[event_index];
            let arrival = u64::from(event_job.arrival);

            if event_job.ticks == 0 {
                completion[event_index] = Some(arrival);
                completed_count += 1;
            } else {
                queues[event_job.level].push_back(event_index);
            }
            next_event += 1;
        }

        let start = now;
        let consumed = remaining[index].min(quanta[level]);
        now += u64::from(consumed);
        remaining[index] -= consumed;
        let finished = remaining[index] == 0;

        runs.push(Run {
            id: job_id,
            start,
            end: now,
            finished,
            level,
        });

        while next_event < order.len()
            && u64::from(jobs[order[next_event]].arrival) <= now
        {
            let event_index = order[next_event];
            let event_job = &jobs[event_index];
            let arrival = u64::from(event_job.arrival);

            if event_job.ticks == 0 {
                completion[event_index] = Some(arrival);
                completed_count += 1;
            } else {
                queues[event_job.level].push_back(event_index);
            }
            next_event += 1;
        }

        if finished {
            completion[index] = Some(now);
            completed_count += 1;
        } else {
            levels[index] = (levels[index] + 1).min(quanta.len() - 1);
            queues[levels[index]].push_back(index);
        }

        last_dispatched_id = Some(job_id);
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
        end: now,
    })
}