#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Job {
    pub id: u32,
    pub arrival: u32,
    pub ticks: u32,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Run {
    pub id: u32,
    pub start: u64,
    pub end: u64,
    pub finished: bool,
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
    Quantum,
    SwitchCost,
    TooManyJobs,
    DuplicateId { index: usize },
    Arrival { index: usize },
    Ticks { index: usize },
}

pub fn schedule(jobs: &[Job], quantum: u32, switch_cost: u32) -> Result<Schedule, Error> {
    if !(1..=1000).contains(&quantum) {
        return Err(Error::Quantum);
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
        if job.ticks > 1000 {
            return Err(Error::Ticks { index });
        }
    }

    let mut events: Vec<usize> = (0..jobs.len()).collect();
    events.sort_by_key(|&index| (jobs[index].arrival, index));

    let mut completion: Vec<(u32, u64)> =
        jobs.iter().map(|job| (job.id, 0)).collect();
    let mut runs: Vec<Run> = Vec::new();
    let mut idle: Vec<(u64, u64)> = Vec::new();
    let mut ready: std::collections::VecDeque<usize> =
        std::collections::VecDeque::new();
    let mut remaining: Vec<u32> = jobs.iter().map(|job| job.ticks).collect();

    let mut time = 0u64;
    let mut event_pos = 0usize;
    let mut completed = 0usize;
    let mut last_dispatched: Option<u32> = None;

    let admit_through = |limit: u64,
                         event_pos: &mut usize,
                         ready: &mut std::collections::VecDeque<usize>,
                         completion: &mut Vec<(u32, u64)>| {
        while *event_pos < events.len()
            && u64::from(jobs[events[*event_pos]].arrival) <= limit
        {
            let index = events[*event_pos];
            if jobs[index].ticks == 0 {
                completion[index].1 = u64::from(jobs[index].arrival);
            } else {
                ready.push_back(index);
            }
            *event_pos += 1;
        }
    };

    while completed < jobs.len() {
        admit_through(time, &mut event_pos, &mut ready, &mut completion);

        if ready.is_empty() {
            if event_pos >= events.len() {
                break;
            }
            let next_time = u64::from(jobs[events[event_pos]].arrival);
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
                last_dispatched = None;
            }
            continue;
        }

        let index = ready.pop_front().unwrap();
        let id = jobs[index].id;

        if last_dispatched.is_some_and(|last| last != id) {
            time += u64::from(switch_cost);
        }
        admit_through(time, &mut event_pos, &mut ready, &mut completion);

        let consumed = remaining[index].min(quantum);
        let start = time;
        time += u64::from(consumed);
        remaining[index] -= consumed;
        let finished = remaining[index] == 0;

        runs.push(Run {
            id,
            start,
            end: time,
            finished,
        });

        admit_through(time, &mut event_pos, &mut ready, &mut completion);

        if finished {
            completion[index].1 = time;
            completed += 1;
        } else {
            ready.push_back(index);
        }
        last_dispatched = Some(id);
    }

    Ok(Schedule {
        runs,
        idle,
        completion,
        end: time,
    })
}