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
    if quantum < 1 || quantum > 1000 {
        return Err(Error::Quantum);
    }
    if switch_cost > 1000 {
        return Err(Error::SwitchCost);
    }
    if jobs.len() > 64 {
        return Err(Error::TooManyJobs);
    }
    for i in 0..jobs.len() {
        for j in 0..i {
            if jobs[i].id == jobs[j].id {
                return Err(Error::DuplicateId { index: i });
            }
        }
        if jobs[i].arrival > 1000 {
            return Err(Error::Arrival { index: i });
        }
        if jobs[i].ticks > 1000 {
            return Err(Error::Ticks { index: i });
        }
    }

    let mut pending: Vec<usize> = (0..jobs.len()).collect();
    pending.sort_by(|&a, &b| {
        jobs[a]
            .arrival
            .cmp(&jobs[b].arrival)
            .then_with(|| a.cmp(&b))
    });

    let mut next = 0usize;
    let mut time = 0u64;
    let mut queue = std::collections::VecDeque::<usize>::new();
    let mut left: Vec<u32> = jobs.iter().map(|j| j.ticks).collect();
    let mut done: Vec<Option<u64>> = vec![None; jobs.len()];
    let mut runs = Vec::new();
    let mut idle = Vec::<(u64, u64)>::new();
    let mut previous: Option<u32> = None;

    while done.iter().any(|v| v.is_none()) {
        while next < pending.len() && u64::from(jobs[pending[next]].arrival) <= time {
            let idx = pending[next];
            next += 1;
            if jobs[idx].ticks == 0 {
                done[idx] = Some(u64::from(jobs[idx].arrival));
            } else {
                queue.push_back(idx);
            }
        }

        if queue.is_empty() {
            if next == pending.len() {
                break;
            }
            let arrival_time = u64::from(jobs[pending[next]].arrival);
            if arrival_time > time {
                if let Some(last) = idle.last_mut() {
                    if last.1 == time {
                        last.1 = arrival_time;
                    } else {
                        idle.push((time, arrival_time));
                    }
                } else {
                    idle.push((time, arrival_time));
                }
                time = arrival_time;
                previous = None;
            }
            continue;
        }

        let idx = queue.pop_front().unwrap();
        let id = jobs[idx].id;
        if previous.map_or(false, |old| old != id) {
            time += u64::from(switch_cost);
        }

        while next < pending.len() && u64::from(jobs[pending[next]].arrival) <= time {
            let arrived = pending[next];
            next += 1;
            if jobs[arrived].ticks == 0 {
                done[arrived] = Some(u64::from(jobs[arrived].arrival));
            } else {
                queue.push_back(arrived);
            }
        }

        let start = time;
        let slice = left[idx].min(quantum);
        time += u64::from(slice);
        left[idx] -= slice;
        let finished = left[idx] == 0;
        runs.push(Run {
            id,
            start,
            end: time,
            finished,
        });

        while next < pending.len() && u64::from(jobs[pending[next]].arrival) <= time {
            let arrived = pending[next];
            next += 1;
            if jobs[arrived].ticks == 0 {
                done[arrived] = Some(u64::from(jobs[arrived].arrival));
            } else {
                queue.push_back(arrived);
            }
        }

        if finished {
            done[idx] = Some(time);
        } else {
            queue.push_back(idx);
        }
        previous = Some(id);
    }

    let completion = jobs
        .iter()
        .enumerate()
        .map(|(i, job)| (job.id, done[i].unwrap()))
        .collect();

    Ok(Schedule {
        runs,
        idle,
        completion,
        end: time,
    })
}
