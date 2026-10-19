use std::collections::VecDeque;

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

    let mut queues: Vec<VecDeque<usize>> =
        (0..quanta.len()).map(|_| VecDeque::new()).collect();
    let mut remaining: Vec<u32> = jobs.iter().map(|job| job.ticks).collect();
    let mut completion: Vec<Option<u64>> = vec![None; jobs.len()];
    let mut event_cursor = 0usize;
    let mut time = 0u64;
    let mut last_dispatched_id: Option<u32> = None;
    let mut runs = Vec::new();
    let mut idle: Vec<(u64, u64)> = Vec::new();
    let mut completed = 0usize;

    let admit_through = |time: u64,
                         event_cursor: &mut usize,
                         queues: &mut Vec<VecDeque<usize>>,
                         completion: &mut Vec<Option<u64>>,
                         completed: &mut usize| {
        while *event_cursor < events.len() {
            let index = events[*event_cursor];
            let job = &jobs[index];
            let arrival = u64::from(job.arrival);
            if arrival > time {
                break;
            }

            *event_cursor += 1;
            if job.ticks == 0 {
                completion[index] = Some(arrival);
                *completed += 1;
            } else {
                queues[job.level].push_back(index);
            }
        }
    };

    while completed < jobs.len() {
        admit_through(
            time,
            &mut event_cursor,
            &mut queues,
            &mut completion,
            &mut completed,
        );

        let Some(selected_level) = queues.iter().position(|queue| !queue.is_empty()) else {
            if event_cursor >= events.len() {
                break;
            }

            let next_index = events[event_cursor];
            let next_time = u64::from(jobs[next_index].arrival);
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

        let index = queues[selected_level]
            .pop_front()
            .expect("selected queue is nonempty");
        let job = &jobs[index];

        if last_dispatched_id.is_some_and(|id| id != job.id) {
            time += u64::from(switch_cost);
        }

        admit_through(
            time,
            &mut event_cursor,
            &mut queues,
            &mut completion,
            &mut completed,
        );

        let consumed = remaining[index].min(quanta[selected_level]);
        let start = time;
        time += u64::from(consumed);
        remaining[index] -= consumed;
        let finished = remaining[index] == 0;

        runs.push(Run {
            id: job.id,
            start,
            end: time,
            finished,
            level: selected_level,
        });

        admit_through(
            time,
            &mut event_cursor,
            &mut queues,
            &mut completion,
            &mut completed,
        );

        if finished {
            completion[index] = Some(time);
            completed += 1;
        } else {
            let next_level = (selected_level + 1).min(quanta.len() - 1);
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