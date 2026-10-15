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
enum EventKind {
    Arrival,
    Wake { job: usize, wake_level: usize },
}

#[derive(Clone, Copy, Debug)]
struct Event {
    time: u64,
    kind_order: u8,
    input_index: usize,
    kind: EventKind,
}

#[derive(Clone, Debug)]
struct State {
    burst_position: usize,
    remaining_cpu: u32,
    level: usize,
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
            kind_order: 0,
            input_index: index,
            kind: EventKind::Arrival,
        });
    }
    events.sort_by_key(|event| (event.time, event.kind_order, event.input_index));

    let mut event_cursor = 0;
    let mut queues: Vec<VecDeque<usize>> =
        (0..quanta.len()).map(|_| VecDeque::new()).collect();
    let mut states: Vec<State> = jobs
        .iter()
        .map(|job| State {
            burst_position: 0,
            remaining_cpu: job.bursts[0],
            level: job.level,
        })
        .collect();
    let mut completion = vec![0u64; jobs.len()];
    let mut runs = Vec::new();
    let mut idle: Vec<(u64, u64)> = Vec::new();
    let mut now = 0u64;
    let mut last_dispatch_id: Option<u32> = None;
    let mut completed_count = 0;

    while completed_count < jobs.len() {
        admit_events(
            now,
            jobs,
            &events,
            &mut event_cursor,
            &mut queues,
            &mut states,
        );

        let Some(level) = queues.iter().position(|queue| !queue.is_empty()) else {
            if event_cursor >= events.len() {
                break;
            }
            let next_time = events[event_cursor].time;
            if next_time > now {
                if let Some(last) = idle.last_mut() {
                    if last.1 == now {
                        last.1 = next_time;
                    } else {
                        idle.push((now, next_time));
                    }
                } else {
                    idle.push((now, next_time));
                }
                now = next_time;
                last_dispatch_id = None;
            }
            continue;
        };

        let job_index = queues[level].pop_front().unwrap();
        let job_id = jobs[job_index].id;

        if last_dispatch_id.is_some_and(|last_id| last_id != job_id) {
            now += switch_cost as u64;
        }

        admit_events(
            now,
            jobs,
            &events,
            &mut event_cursor,
            &mut queues,
            &mut states,
        );

        let quantum = quanta[level];
        let consumed = states[job_index].remaining_cpu.min(quantum);
        let start = now;
        now += consumed as u64;
        let end = now;
        states[job_index].remaining_cpu -= consumed;

        let burst_finished = states[job_index].remaining_cpu == 0;
        let job_finished = burst_finished
            && states[job_index].burst_position + 1 == jobs[job_index].bursts.len();

        runs.push(Run {
            id: job_id,
            start,
            end,
            finished: job_finished,
            level,
        });

        if burst_finished && !job_finished {
            let io_position = states[job_index].burst_position + 1;
            let wake_time = end + jobs[job_index].bursts[io_position] as u64;
            let wake_level = level.saturating_sub(1);
            let event = Event {
                time: wake_time,
                kind_order: 1,
                input_index: job_index,
                kind: EventKind::Wake {
                    job: job_index,
                    wake_level,
                },
            };
            let insertion = events[event_cursor..]
                .partition_point(|existing| {
                    (existing.time, existing.kind_order, existing.input_index)
                        <= (event.time, event.kind_order, event.input_index)
                })
                + event_cursor;
            events.insert(insertion, event);

            states[job_index].burst_position += 2;
            states[job_index].remaining_cpu =
                jobs[job_index].bursts[states[job_index].burst_position];
        }

        admit_events(
            end,
            jobs,
            &events,
            &mut event_cursor,
            &mut queues,
            &mut states,
        );

        if job_finished {
            completion[job_index] = end;
            completed_count += 1;
        } else if !burst_finished {
            states[job_index].level = (level + 1).min(quanta.len() - 1);
            queues[states[job_index].level].push_back(job_index);
        }

        last_dispatch_id = Some(job_id);
    }

    let mut stats = Vec::with_capacity(jobs.len());
    let mut total_waiting = 0u64;
    let mut total_turnaround = 0u64;

    for (index, job) in jobs.iter().enumerate() {
        let turnaround = completion[index] - job.arrival as u64;
        let cpu_total: u64 = job
            .bursts
            .iter()
            .step_by(2)
            .map(|&length| length as u64)
            .sum();
        let io_total: u64 = job
            .bursts
            .iter()
            .skip(1)
            .step_by(2)
            .map(|&length| length as u64)
            .sum();
        let waiting = turnaround - cpu_total - io_total;

        total_waiting += waiting;
        total_turnaround += turnaround;
        stats.push(Stat {
            id: job.id,
            completion: completion[index],
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
        end: now,
    })
}

fn admit_events(
    through: u64,
    jobs: &[Job],
    events: &[Event],
    event_cursor: &mut usize,
    queues: &mut [VecDeque<usize>],
    states: &mut [State],
) {
    while *event_cursor < events.len() && events[*event_cursor].time <= through {
        let event = events[*event_cursor];
        *event_cursor += 1;
        match event.kind {
            EventKind::Arrival => {
                let index = event.input_index;
                queues[jobs[index].level].push_back(index);
            }
            EventKind::Wake { job, wake_level } => {
                states[job].level = wake_level;
                queues[wake_level].push_back(job);
            }
        }
    }
}