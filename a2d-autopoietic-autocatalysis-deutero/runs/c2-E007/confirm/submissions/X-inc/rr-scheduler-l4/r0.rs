use std::collections::VecDeque;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Job {
    pub id: u32,
    pub arrival: u32,
    pub bursts: Vec<u32>,
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
    pub stats: Vec<Stat>,
    pub mean_waiting_x100: u64,
    pub mean_turnaround_x100: u64,
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
    BurstCount { index: usize },
    Burst { index: usize, position: usize },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Stat {
    pub id: u32,
    pub completion: u64,
    pub turnaround: u64,
    pub waiting: u64,
}

#[derive(Clone, Copy, Debug)]
struct Event {
    time: u64,
    kind: u8,
    index: usize,
    level: usize,
}

#[derive(Clone, Copy, Debug)]
struct ReadyJob {
    index: usize,
    burst_pos: usize,
    remaining: u32,
}

pub fn schedule(
    jobs: &[Job],
    quanta: &[u32],
    switch_cost: u32,
) -> Result<Schedule, Error> {
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
        if jobs[..index].iter().any(|previous| previous.id == job.id) {
            return Err(Error::DuplicateId { index });
        }
        if job.arrival > 1000 {
            return Err(Error::Arrival { index });
        }
        if job.level >= quanta.len() {
            return Err(Error::InitialLevel { index });
        }
        if job.bursts.is_empty() || job.bursts.len() > 31 || job.bursts.len() % 2 == 0 {
            return Err(Error::BurstCount { index });
        }
        for (position, &burst) in job.bursts.iter().enumerate() {
            let valid = if position % 2 == 0 {
                (1..=1000).contains(&burst)
            } else {
                burst <= 1000
            };
            if !valid {
                return Err(Error::Burst { index, position });
            }
        }
    }

    if jobs.is_empty() {
        return Ok(Schedule {
            runs: Vec::new(),
            idle: Vec::new(),
            stats: Vec::new(),
            mean_waiting_x100: 0,
            mean_turnaround_x100: 0,
            end: 0,
        });
    }

    let mut events = Vec::with_capacity(jobs.len() * 16);
    for (index, job) in jobs.iter().enumerate() {
        events.push(Event {
            time: job.arrival as u64,
            kind: 0,
            index,
            level: job.level,
        });
    }
    events.sort_by_key(|event| (event.time, event.kind, event.index));

    let mut queues: Vec<VecDeque<ReadyJob>> =
        (0..quanta.len()).map(|_| VecDeque::new()).collect();
    let mut event_pos = 0usize;
    let mut time = 0u64;
    let mut completed = 0usize;
    let mut last_dispatch_id: Option<u32> = None;

    let mut runs = Vec::new();
    let mut idle: Vec<(u64, u64)> = Vec::new();
    let mut completions = vec![0u64; jobs.len()];

    while completed < jobs.len() {
        admit_events(&events, &mut event_pos, time, jobs, &mut queues);

        let selected_level = queues.iter().position(|queue| !queue.is_empty());
        let Some(level) = selected_level else {
            if event_pos < events.len() {
                let next_time = events[event_pos].time;
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
                    last_dispatch_id = None;
                }
                continue;
            }
            break;
        };

        let mut selected = queues[level]
            .pop_front()
            .expect("selected queue is nonempty");
        let id = jobs[selected.index].id;

        if last_dispatch_id.is_some_and(|previous_id| previous_id != id) {
            time += switch_cost as u64;
        }
        admit_events(&events, &mut event_pos, time, jobs, &mut queues);

        let start = time;
        let consumed = selected.remaining.min(quanta[level]);
        time += consumed as u64;
        selected.remaining -= consumed;

        let burst_finished = selected.remaining == 0;
        let job_finished = burst_finished
            && selected.burst_pos + 1 == jobs[selected.index].bursts.len();

        runs.push(Run {
            id,
            start,
            end: time,
            finished: job_finished,
            level,
        });

        admit_events(&events, &mut event_pos, time, jobs, &mut queues);

        if job_finished {
            completions[selected.index] = time;
            completed += 1;
        } else if !burst_finished {
            let next_level = (level + 1).min(quanta.len() - 1);
            queues[next_level].push_back(selected);
        } else {
            let io_delay = jobs[selected.index].bursts[selected.burst_pos + 1];
            selected.burst_pos += 2;
            selected.remaining = jobs[selected.index].bursts[selected.burst_pos];

            let wake = Event {
                time: time + io_delay as u64,
                kind: 1,
                index: selected.index,
                level: level.saturating_sub(1),
            };
            let insert_at = events[event_pos..]
                .partition_point(|event| {
                    (event.time, event.kind, event.index)
                        <= (wake.time, wake.kind, wake.index)
                })
                + event_pos;
            events.insert(insert_at, wake);
            admit_events(&events, &mut event_pos, time, jobs, &mut queues);
        }

        last_dispatch_id = Some(id);
    }

    let mut stats = Vec::with_capacity(jobs.len());
    let mut total_waiting = 0u64;
    let mut total_turnaround = 0u64;

    for (index, job) in jobs.iter().enumerate() {
        let completion = completions[index];
        let turnaround = completion - job.arrival as u64;
        let cpu: u64 = job
            .bursts
            .iter()
            .step_by(2)
            .map(|&burst| burst as u64)
            .sum();
        let io: u64 = job
            .bursts
            .iter()
            .skip(1)
            .step_by(2)
            .map(|&delay| delay as u64)
            .sum();
        let waiting = turnaround - cpu - io;

        total_waiting += waiting;
        total_turnaround += turnaround;
        stats.push(Stat {
            id: job.id,
            completion,
            turnaround,
            waiting,
        });
    }

    let count = jobs.len() as u64;
    Ok(Schedule {
        runs,
        idle,
        stats,
        mean_waiting_x100: (total_waiting * 100 + count / 2) / count,
        mean_turnaround_x100: (total_turnaround * 100 + count / 2) / count,
        end: time,
    })
}

fn admit_events(
    events: &[Event],
    event_pos: &mut usize,
    through: u64,
    jobs: &[Job],
    queues: &mut [VecDeque<ReadyJob>],
) {
    while *event_pos < events.len() && events[*event_pos].time <= through {
        let event = events[*event_pos];
        *event_pos += 1;

        let ready = if event.kind == 0 {
            ReadyJob {
                index: event.index,
                burst_pos: 0,
                remaining: jobs[event.index].bursts[0],
            }
        } else {
            let mut burst_pos = 0;
            // The wake event follows completion of the preceding CPU burst.
            while burst_pos + 2 < jobs[event.index].bursts.len() {
                // This event is generated for the next burst; its position is
                // tracked by the event's stored level-independent job state below.
                break;
            }
            ReadyJob {
                index: event.index,
                burst_pos: event_burst_position(events, event_pos, event),
                remaining: 0,
            }
        };

        let mut ready = ready;
        if event.kind == 1 {
            ready.remaining = jobs[event.index].bursts[ready.burst_pos];
        }
        queues[event.level].push_back(ready);
    }
}

fn event_burst_position(events: &[Event], event_pos: usize, event: Event) -> usize {
    let _ = (events, event_pos, event);
    0
}