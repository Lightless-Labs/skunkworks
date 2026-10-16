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

    let mut remaining: Vec<u32> = jobs.iter().map(|job| job.ticks).collect();
    let mut completion_times = vec![None; jobs.len()];
    let mut ready = std::collections::VecDeque::new();
    let mut event_index = 0;
    let mut time = 0u64;
    let mut last_dispatched: Option<u32> = None;
    let mut runs = Vec::new();
    let mut idle: Vec<(u64, u64)> = Vec::new();

    fn admit(
        jobs: &[Job],
        events: &[usize],
        event_index: &mut usize,
        time: u64,
        ready: &mut std::collections::VecDeque<usize>,
        completion_times: &mut [Option<u64>],
    ) {
        while *event_index < events.len() {
            let index = events[*event_index];
            let arrival = u64::from(jobs[index].arrival);
            if arrival > time {
                break;
            }
            *event_index += 1;
            if jobs[index].ticks == 0 {
                completion_times[index] = Some(arrival);
            } else {
                ready.push_back(index);
            }
        }
    }

    while completion_times.iter().any(Option::is_none) {
        admit(
            jobs,
            &events,
            &mut event_index,
            time,
            &mut ready,
            &mut completion_times,
        );

        if ready.is_empty() {
            if event_index >= events.len() {
                break;
            }
            let next_time = u64::from(jobs[events[event_index]].arrival);
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

        let selected = ready.pop_front().expect("ready queue was nonempty");
        let id = jobs[selected].id;

        if last_dispatched.is_some_and(|last_id| last_id != id) {
            time += u64::from(switch_cost);
        }

        admit(
            jobs,
            &events,
            &mut event_index,
            time,
            &mut ready,
            &mut completion_times,
        );

        let consumed = remaining[selected].min(quantum);
        let start = time;
        time += u64::from(consumed);
        remaining[selected] -= consumed;
        let finished = remaining[selected] == 0;

        runs.push(Run {
            id,
            start,
            end: time,
            finished,
        });

        admit(
            jobs,
            &events,
            &mut event_index,
            time,
            &mut ready,
            &mut completion_times,
        );

        if finished {
            completion_times[selected] = Some(time);
        } else {
            ready.push_back(selected);
        }

        last_dispatched = Some(id);
    }

    let completion = jobs
        .iter()
        .enumerate()
        .map(|(index, job)| {
            (
                job.id,
                completion_times[index].expect("every job must complete"),
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