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

    let mut arrival_order: Vec<usize> = (0..jobs.len()).collect();
    arrival_order.sort_by_key(|&index| (jobs[index].arrival, index));

    let mut runs = Vec::new();
    let mut idle = Vec::new();
    let mut completion_times = vec![0u64; jobs.len()];
    let mut completed = vec![false; jobs.len()];
    let mut remaining: Vec<u32> = jobs.iter().map(|job| job.ticks).collect();
    let mut ready = std::collections::VecDeque::new();

    let mut time = 0u64;
    let mut next_arrival = 0usize;
    let mut completed_count = 0usize;
    let mut last_dispatched_id: Option<u32> = None;

    while completed_count < jobs.len() {
        while next_arrival < arrival_order.len()
            && u64::from(jobs[arrival_order[next_arrival]].arrival) <= time
        {
            let index = arrival_order[next_arrival];
            next_arrival += 1;

            if jobs[index].ticks == 0 {
                completed[index] = true;
                completion_times[index] = u64::from(jobs[index].arrival);
                completed_count += 1;
            } else {
                ready.push_back(index);
            }
        }

        if completed_count == jobs.len() {
            break;
        }

        if ready.is_empty() {
            if next_arrival == arrival_order.len() {
                break;
            }

            let next_time = u64::from(jobs[arrival_order[next_arrival]].arrival);
            if next_time > time {
                if let Some((_, end)) = idle.last_mut() {
                    if *end == time {
                        *end = next_time;
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

        let index = ready.pop_front().expect("ready queue is nonempty");
        let id = jobs[index].id;

        if last_dispatched_id.is_some_and(|last_id| last_id != id) {
            time += u64::from(switch_cost);
        }

        while next_arrival < arrival_order.len()
            && u64::from(jobs[arrival_order[next_arrival]].arrival) <= time
        {
            let arrival_index = arrival_order[next_arrival];
            next_arrival += 1;

            if jobs[arrival_index].ticks == 0 {
                completed[arrival_index] = true;
                completion_times[arrival_index] =
                    u64::from(jobs[arrival_index].arrival);
                completed_count += 1;
            } else {
                ready.push_back(arrival_index);
            }
        }

        let start = time;
        let consumed = remaining[index].min(quantum);
        time += u64::from(consumed);
        remaining[index] -= consumed;
        let finished = remaining[index] == 0;

        runs.push(Run {
            id,
            start,
            end: time,
            finished,
        });

        while next_arrival < arrival_order.len()
            && u64::from(jobs[arrival_order[next_arrival]].arrival) <= time
        {
            let arrival_index = arrival_order[next_arrival];
            next_arrival += 1;

            if jobs[arrival_index].ticks == 0 {
                completed[arrival_index] = true;
                completion_times[arrival_index] =
                    u64::from(jobs[arrival_index].arrival);
                completed_count += 1;
            } else {
                ready.push_back(arrival_index);
            }
        }

        if finished {
            completed[index] = true;
            completion_times[index] = time;
            completed_count += 1;
        } else {
            ready.push_back(index);
        }

        last_dispatched_id = Some(id);
    }

    let completion = jobs
        .iter()
        .enumerate()
        .map(|(index, job)| (job.id, completion_times[index]))
        .collect();

    Ok(Schedule {
        runs,
        idle,
        completion,
        end: time,
    })
}