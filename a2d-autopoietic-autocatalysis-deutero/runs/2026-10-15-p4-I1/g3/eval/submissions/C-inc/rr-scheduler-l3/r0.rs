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

    let mut arrivals: Vec<usize> = (0..jobs.len()).collect();
    arrivals.sort_by_key(|&index| (jobs[index].arrival, index));

    let mut ready: Vec<std::collections::VecDeque<usize>> = (0..quanta.len())
        .map(|_| std::collections::VecDeque::new())
        .collect();
    let mut remaining: Vec<u32> = jobs.iter().map(|job| job.ticks).collect();
    let mut completion: Vec<Option<u64>> = vec![None; jobs.len()];
    let mut runs = Vec::new();
    let mut idle: Vec<(u64, u64)> = Vec::new();

    let mut time = 0u64;
    let mut arrival_cursor = 0usize;
    let mut completed_count = 0usize;
    let mut last_dispatched_id: Option<u32> = None;

    let admit_through = |time: u64,
                         arrival_cursor: &mut usize,
                         ready: &mut Vec<std::collections::VecDeque<usize>>,
                         completion: &mut Vec<Option<u64>>,
                         completed_count: &mut usize| {
        while *arrival_cursor < arrivals.len()
            && u64::from(jobs[arrivals[*arrival_cursor]].arrival) <= time
        {
            let index = arrivals[*arrival_cursor];
            let job = &jobs[index];

            if job.ticks == 0 {
                completion[index] = Some(u64::from(job.arrival));
                *completed_count += 1;
            } else {
                ready[job.level].push_back(index);
            }

            *arrival_cursor += 1;
        }
    };

    while completed_count < jobs.len() {
        admit_through(
            time,
            &mut arrival_cursor,
            &mut ready,
            &mut completion,
            &mut completed_count,
        );

        let Some(queue_level) = ready.iter().position(|queue| !queue.is_empty()) else {
            if arrival_cursor >= arrivals.len() {
                break;
            }

            let next_time = u64::from(jobs[arrivals[arrival_cursor]].arrival);
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

        let index = ready[queue_level]
            .pop_front()
            .expect("selected queue is nonempty");
        let job_id = jobs[index].id;

        if last_dispatched_id.is_some_and(|last_id| last_id != job_id) {
            time += u64::from(switch_cost);
        }

        admit_through(
            time,
            &mut arrival_cursor,
            &mut ready,
            &mut completion,
            &mut completed_count,
        );

        let start = time;
        let consumed = remaining[index].min(quanta[queue_level]);
        time += u64::from(consumed);
        remaining[index] -= consumed;
        let finished = remaining[index] == 0;

        runs.push(Run {
            id: job_id,
            start,
            end: time,
            finished,
            level: queue_level,
        });

        admit_through(
            time,
            &mut arrival_cursor,
            &mut ready,
            &mut completion,
            &mut completed_count,
        );

        if finished {
            completion[index] = Some(time);
            completed_count += 1;
        } else {
            let next_level = (queue_level + 1).min(quanta.len() - 1);
            ready[next_level].push_back(index);
        }

        last_dispatched_id = Some(job_id);
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