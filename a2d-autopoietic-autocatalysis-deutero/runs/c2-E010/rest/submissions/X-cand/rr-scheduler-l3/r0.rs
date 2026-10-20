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
    use std::collections::VecDeque;

    if quanta.is_empty() || quanta.len() > 8 {
        return Err(Error::LevelCount);
    }
    for level in 0..quanta.len() {
        if quanta[level] == 0 || quanta[level] > 1000 {
            return Err(Error::Quantum { level });
        }
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
        if jobs[i].level >= quanta.len() {
            return Err(Error::InitialLevel { index: i });
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

    let mut queues: Vec<VecDeque<usize>> =
        (0..quanta.len()).map(|_| VecDeque::new()).collect();
    let mut remaining: Vec<u32> = jobs.iter().map(|job| job.ticks).collect();
    let mut level: Vec<usize> = jobs.iter().map(|job| job.level).collect();
    let mut completion: Vec<Option<u64>> = vec![None; jobs.len()];
    let mut runs = Vec::new();
    let mut idle: Vec<(u64, u64)> = Vec::new();
    let mut cursor = 0usize;
    let mut time = 0u64;
    let mut last_id: Option<u32> = None;
    let mut done = 0usize;

    while done != jobs.len() {
        while cursor < pending.len() && u64::from(jobs[pending[cursor]].arrival) <= time {
            let i = pending[cursor];
            cursor += 1;
            if jobs[i].ticks == 0 {
                completion[i] = Some(u64::from(jobs[i].arrival));
                done += 1;
            } else {
                queues[jobs[i].level].push_back(i);
            }
        }

        let chosen_queue = (0..queues.len()).find(|&q| !queues[q].is_empty());
        if chosen_queue.is_none() {
            if cursor == pending.len() {
                break;
            }
            let next = u64::from(jobs[pending[cursor]].arrival);
            if next > time {
                if let Some(interval) = idle.last_mut() {
                    if interval.1 == time {
                        interval.1 = next;
                    } else {
                        idle.push((time, next));
                    }
                } else {
                    idle.push((time, next));
                }
                time = next;
                last_id = None;
            }
            continue;
        }

        let q = chosen_queue.unwrap();
        let i = queues[q].pop_front().unwrap();
        let id = jobs[i].id;
        if last_id.map_or(false, |previous| previous != id) {
            time += u64::from(switch_cost);
        }

        while cursor < pending.len() && u64::from(jobs[pending[cursor]].arrival) <= time {
            let arriving = pending[cursor];
            cursor += 1;
            if jobs[arriving].ticks == 0 {
                completion[arriving] = Some(u64::from(jobs[arriving].arrival));
                done += 1;
            } else {
                queues[jobs[arriving].level].push_back(arriving);
            }
        }

        let run_level = level[i];
        let amount = remaining[i].min(quanta[run_level]);
        let start = time;
        time += u64::from(amount);
        remaining[i] -= amount;
        let finished = remaining[i] == 0;

        runs.push(Run {
            id,
            start,
            end: time,
            finished,
            level: run_level,
        });

        while cursor < pending.len() && u64::from(jobs[pending[cursor]].arrival) <= time {
            let arriving = pending[cursor];
            cursor += 1;
            if jobs[arriving].ticks == 0 {
                completion[arriving] = Some(u64::from(jobs[arriving].arrival));
                done += 1;
            } else {
                queues[jobs[arriving].level].push_back(arriving);
            }
        }

        if finished {
            completion[i] = Some(time);
            done += 1;
        } else {
            level[i] = (run_level + 1).min(quanta.len() - 1);
            queues[level[i]].push_back(i);
        }
        last_id = Some(id);
    }

    let completion = jobs
        .iter()
        .enumerate()
        .map(|(i, job)| (job.id, completion[i].unwrap()))
        .collect();

    Ok(Schedule {
        runs,
        idle,
        completion,
        end: time,
    })
}
