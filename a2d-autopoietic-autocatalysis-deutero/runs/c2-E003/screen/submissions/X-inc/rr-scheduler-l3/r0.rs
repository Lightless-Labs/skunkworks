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

    let mut events: Vec<usize> = (0..jobs.len()).collect();
    events.sort_by_key(|&index| (jobs[index].arrival, index));

    let mut next_event = 0;
    let mut queues: Vec<std::collections::VecDeque<usize>> = (0..quanta.len())
        .map(|_| std::collections::VecDeque::new())
        .collect();
    let mut remaining: Vec<u32> = jobs.iter().map(|job| job.ticks).collect();
    let mut completion: Vec<Option<u64>> = vec![None; jobs.len()];
    let mut runs = Vec::new();
    let mut idle: Vec<(u64, u64)> = Vec::new();
    let mut time = 0u64;
    let mut last_dispatched_id: Option<u32> = None;
    let mut completed_count = 0usize;

    fn admit(
        jobs: &[Job],
        events: &[usize],
        next_event: &mut usize,
        time: u64,
        queues: &mut [std::collections::VecDeque<usize>],
        completion: &mut [Option<u64>],
        completed_count: &mut usize,
    ) {
        while *next_event < events.len()
            && u64::from(jobs[events[*next_event]].arrival) <= time
        {
            let index = events[*next_event];
            *next_event += 1;

            if jobs[index].ticks == 0 {
                completion[index] = Some(u64::from(jobs[index].arrival));
                *completed_count += 1;
            } else {
                queues[jobs[index].level].push_back(index);
            }
        }
    }

    while completed_count < jobs.len() {
        admit(
            jobs,
            &events,
            &mut next_event,
            time,
            &mut queues,
            &mut completion,
            &mut completed_count,
        );

        let Some(level) = queues.iter().position(|queue| !queue.is_empty()) else {
            if next_event < events.len() {
                let next_time = u64::from(jobs[events[next_event]].arrival);
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
            }
            break;
        };

        let index = queues[level]
            .pop_front()
            .expect("selected queue is nonempty");
        let job = &jobs[index];

        if last_dispatched_id.is_some_and(|id| id != job.id) {
            time += u64::from(switch_cost);
        }

        admit(
            jobs,
            &events,
            &mut next_event,
            time,
            &mut queues,
            &mut completion,
            &mut completed_count,
        );

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

        admit(
            jobs,
            &events,
            &mut next_event,
            time,
            &mut queues,
            &mut completion,
            &mut completed_count,
        );

        if finished {
            completion[index] = Some(time);
            completed_count += 1;
        } else {
            let next_level = (level + 1).min(quanta.len() - 1);
            queues[next_level].push_back(index);
        }

        last_dispatched_id = Some(job.id);
    }

    let completion = jobs
        .iter()
        .enumerate()
        .map(|(index, job)| (job.id, completion[index].unwrap_or(0)))
        .collect();

    Ok(Schedule {
        runs,
        idle,
        completion,
        end: time,
    })
}