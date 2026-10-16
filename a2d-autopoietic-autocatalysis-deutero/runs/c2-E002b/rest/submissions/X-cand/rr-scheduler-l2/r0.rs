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

pub fn schedule(
    jobs: &[Job],
    quantum: u32,
    switch_cost: u32,
) -> Result<Schedule, Error> {
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

    let mut arrival_order: Vec<usize> = (0..jobs.len()).collect();
    arrival_order.sort_by_key(|&index| (jobs[index].arrival, index));

    let mut next_arrival = 0usize;
    let mut ready: VecDeque<usize> = VecDeque::new();
    let mut remaining: Vec<u32> = jobs.iter().map(|job| job.ticks).collect();
    let mut completion_times: Vec<Option<u64>> = vec![None; jobs.len()];
    let mut runs = Vec::new();
    let mut idle = Vec::new();
    let mut time = 0u64;
    let mut last_dispatched: Option<u32> = None;

    fn admit(
        jobs: &[Job],
        arrival_order: &[usize],
        next_arrival: &mut usize,
        ready: &mut VecDeque<usize>,
        completion_times: &mut [Option<u64>],
        through: u64,
    ) {
        while *next_arrival < arrival_order.len() {
            let index = arrival_order[*next_arrival];
            let job = &jobs[index];
            if u64::from(job.arrival) > through {
                break;
            }
            *next_arrival += 1;
            if job.ticks == 0 {
                completion_times[index] = Some(u64::from(job.arrival));
            } else {
                ready.push_back(index);
            }
        }
    }

    while completion_times.iter().any(Option::is_none) {
        admit(
            jobs,
            &arrival_order,
            &mut next_arrival,
            &mut ready,
            &mut completion_times,
            time,
        );

        if ready.is_empty() {
            if next_arrival >= arrival_order.len() {
                break;
            }
            let next_time = u64::from(jobs[arrival_order[next_arrival]].arrival);
            if next_time > time {
                if let Some((_, previous_end)) = idle.last_mut() {
                    if *previous_end == time {
                        *previous_end = next_time;
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

        let index = ready.pop_front().expect("ready queue is nonempty");
        let job_id = jobs[index].id;

        if last_dispatched.is_some_and(|previous| previous != job_id) {
            time += u64::from(switch_cost);
        }

        admit(
            jobs,
            &arrival_order,
            &mut next_arrival,
            &mut ready,
            &mut completion_times,
            time,
        );

        let consumed = remaining[index].min(quantum);
        let start = time;
        time += u64::from(consumed);
        remaining[index] -= consumed;
        let finished = remaining[index] == 0;

        runs.push(Run {
            id: job_id,
            start,
            end: time,
            finished,
        });

        admit(
            jobs,
            &arrival_order,
            &mut next_arrival,
            &mut ready,
            &mut completion_times,
            time,
        );

        if finished {
            completion_times[index] = Some(time);
        } else {
            ready.push_back(index);
        }
        last_dispatched = Some(job_id);
    }

    let completion = jobs
        .iter()
        .enumerate()
        .map(|(index, job)| {
            (
                job.id,
                completion_times[index].expect("every job has completed"),
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

use std::collections::VecDeque;