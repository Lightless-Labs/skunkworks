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

    let mut arrival_order: Vec<usize> = (0..jobs.len()).collect();
    arrival_order.sort_by_key(|&index| (jobs[index].arrival, index));

    let mut ready: Vec<VecDeque<usize>> =
        (0..quanta.len()).map(|_| VecDeque::new()).collect();
    let mut remaining: Vec<u32> = jobs.iter().map(|job| job.ticks).collect();
    let mut levels: Vec<usize> = jobs.iter().map(|job| job.level).collect();
    let mut completion: Vec<Option<u64>> = vec![None; jobs.len()];
    let mut runs: Vec<Run> = Vec::new();
    let mut idle: Vec<(u64, u64)> = Vec::new();

    let mut arrival_cursor = 0;
    let mut time = 0_u64;
    let mut last_dispatched_id: Option<u32> = None;

    fn admit_through(
        jobs: &[Job],
        arrival_order: &[usize],
        cursor: &mut usize,
        ready: &mut [VecDeque<usize>],
        remaining: &[u32],
        completion: &mut [Option<u64>],
        through: u64,
    ) {
        while *cursor < arrival_order.len() {
            let index = arrival_order[*cursor];
            let job = &jobs[index];
            if u64::from(job.arrival) > through {
                break;
            }
            *cursor += 1;

            if remaining[index] == 0 {
                completion[index] = Some(u64::from(job.arrival));
            } else {
                ready[job.level].push_back(index);
            }
        }
    }

    while completion.iter().any(Option::is_none) {
        admit_through(
            jobs,
            &arrival_order,
            &mut arrival_cursor,
            &mut ready,
            &remaining,
            &mut completion,
            time,
        );

        let selected = ready.iter_mut().find_map(VecDeque::pop_front);

        let Some(index) = selected else {
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

        let job = &jobs[index];
        if last_dispatched_id.is_some_and(|last_id| last_id != job.id) {
            time += u64::from(switch_cost);
        }

        admit_through(
            jobs,
            &arrival_order,
            &mut arrival_cursor,
            &mut ready,
            &remaining,
            &mut completion,
            time,
        );

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

        admit_through(
            jobs,
            &arrival_order,
            &mut arrival_cursor,
            &mut ready,
            &remaining,
            &mut completion,
            time,
        );

        if finished {
            completion[index] = Some(time);
        } else {
            levels[index] = (level + 1).min(quanta.len() - 1);
            ready[levels[index]].push_back(index);
        }

        last_dispatched_id = Some(job.id);
    }

    let completion = jobs
        .iter()
        .enumerate()
        .map(|(index, job)| (job.id, completion[index].unwrap_or(time)))
        .collect();

    Ok(Schedule {
        runs,
        idle,
        completion,
        end: time,
    })
}

fn main() {}