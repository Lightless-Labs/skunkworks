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

pub fn schedule(
    jobs: &[Job],
    quanta: &[u32],
    switch_cost: u32,
) -> Result<Schedule, Error> {
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

    let mut event_order: Vec<usize> = (0..jobs.len()).collect();
    event_order.sort_by_key(|&index| (jobs[index].arrival, index));

    let mut queues: Vec<std::collections::VecDeque<usize>> =
        (0..quanta.len()).map(|_| std::collections::VecDeque::new()).collect();
    let mut remaining: Vec<u32> = jobs.iter().map(|job| job.ticks).collect();
    let mut completion_times: Vec<Option<u64>> = vec![None; jobs.len()];
    let mut runs = Vec::new();
    let mut idle: Vec<(u64, u64)> = Vec::new();

    let mut event_cursor = 0;
    let mut time = 0u64;
    let mut last_dispatched_id: Option<u32> = None;
    let mut completed_count = 0usize;

    while completed_count < jobs.len() {
        while event_cursor < event_order.len()
            && u64::from(jobs[event_order[event_cursor]].arrival) <= time
        {
            let index = event_order[event_cursor];
            let job = &jobs[index];
            if job.ticks == 0 {
                completion_times[index] = Some(u64::from(job.arrival));
                completed_count += 1;
            } else {
                queues[job.level].push_back(index);
            }
            event_cursor += 1;
        }

        let selected_level = queues.iter().position(|queue| !queue.is_empty());

        if let Some(level) = selected_level {
            let index = queues[level].pop_front().expect("selected queue is nonempty");
            let job = &jobs[index];

            if last_dispatched_id.is_some_and(|id| id != job.id) {
                time += u64::from(switch_cost);
            }

            while event_cursor < event_order.len()
                && u64::from(jobs[event_order[event_cursor]].arrival) <= time
            {
                let arriving_index = event_order[event_cursor];
                let arriving_job = &jobs[arriving_index];
                if arriving_job.ticks == 0 {
                    completion_times[arriving_index] = Some(u64::from(arriving_job.arrival));
                    completed_count += 1;
                } else {
                    queues[arriving_job.level].push_back(arriving_index);
                }
                event_cursor += 1;
            }

            let start = time;
            let consumed = remaining[index].min(quanta[level]);
            time += u64::from(consumed);
            let finished = consumed == remaining[index];
            remaining[index] -= consumed;

            runs.push(Run {
                id: job.id,
                start,
                end: time,
                finished,
                level,
            });

            while event_cursor < event_order.len()
                && u64::from(jobs[event_order[event_cursor]].arrival) <= time
            {
                let arriving_index = event_order[event_cursor];
                let arriving_job = &jobs[arriving_index];
                if arriving_job.ticks == 0 {
                    completion_times[arriving_index] = Some(u64::from(arriving_job.arrival));
                    completed_count += 1;
                } else {
                    queues[arriving_job.level].push_back(arriving_index);
                }
                event_cursor += 1;
            }

            if finished {
                completion_times[index] = Some(time);
                completed_count += 1;
            } else {
                let next_level = (level + 1).min(quanta.len() - 1);
                queues[next_level].push_back(index);
            }

            last_dispatched_id = Some(job.id);
        } else if event_cursor < event_order.len() {
            let next_time = u64::from(jobs[event_order[event_cursor]].arrival);
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
        }
    }

    let completion = jobs
        .iter()
        .enumerate()
        .map(|(index, job)| {
            (
                job.id,
                completion_times[index].expect("every job completes"),
            )
        })
        .collect();

    Ok(Schedule {
        runs,
        idle,
        completion,
        end: time,
    })
}