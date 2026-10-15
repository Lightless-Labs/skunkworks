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
    let mut completion_times: Vec<Option<u64>> = vec![None; jobs.len()];
    let mut runs: Vec<Run> = Vec::new();
    let mut idle: Vec<(u64, u64)> = Vec::new();
    let mut ready = std::collections::VecDeque::new();
    let mut event_index = 0usize;
    let mut time = 0u64;
    let mut last_dispatched_id: Option<u32> = None;

    let admit_through = |time: u64,
                         event_index: &mut usize,
                         ready: &mut std::collections::VecDeque<usize>,
                         completion_times: &mut Vec<Option<u64>>| {
        while *event_index < events.len() {
            let index = events[*event_index];
            let arrival = jobs[index].arrival as u64;
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
    };

    loop {
        admit_through(
            time,
            &mut event_index,
            &mut ready,
            &mut completion_times,
        );

        if let Some(index) = ready.pop_front() {
            let id = jobs[index].id;
            if last_dispatched_id.is_some_and(|last_id| last_id != id) {
                time += switch_cost as u64;
            }

            admit_through(
                time,
                &mut event_index,
                &mut ready,
                &mut completion_times,
            );

            let consumed = remaining[index].min(quantum);
            let start = time;
            time += consumed as u64;
            remaining[index] -= consumed;
            let finished = remaining[index] == 0;

            runs.push(Run {
                id,
                start,
                end: time,
                finished,
            });

            admit_through(
                time,
                &mut event_index,
                &mut ready,
                &mut completion_times,
            );

            if finished {
                completion_times[index] = Some(time);
            } else {
                ready.push_back(index);
            }

            last_dispatched_id = Some(id);
        } else if event_index < events.len() {
            let next_time = jobs[events[event_index]].arrival as u64;
            if next_time > time {
                if let Some(last_idle) = idle.last_mut() {
                    if last_idle.1 == time {
                        last_idle.1 = next_time;
                    } else {
                        idle.push((time, next_time));
                    }
                } else {
                    idle.push((time, next_time));
                }
                time = next_time;
                last_dispatched_id = None;
            }
        } else {
            break;
        }
    }

    let completion: Vec<(u32, u64)> = jobs
        .iter()
        .zip(completion_times)
        .map(|(job, completed_at)| (job.id, completed_at.expect("all jobs complete")))
        .collect();

    Ok(Schedule {
        runs,
        idle,
        completion,
        end: time,
    })
}

fn main() {}