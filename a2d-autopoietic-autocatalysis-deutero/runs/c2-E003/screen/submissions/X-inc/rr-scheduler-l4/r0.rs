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
    Wake { level: usize },
}

#[derive(Clone, Copy, Debug)]
struct Event {
    time: u64,
    kind: EventKind,
    index: usize,
}

#[derive(Clone, Debug)]
struct State {
    burst_index: usize,
    remaining: u32,
    level: usize,
    completion: Option<u64>,
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

    let n = jobs.len();
    if n == 0 {
        return Ok(Schedule {
            runs: Vec::new(),
            idle: Vec::new(),
            stats: Vec::new(),
            mean_waiting_x100: 0,
            mean_turnaround_x100: 0,
            end: 0,
        });
    }

    let mut events: Vec<Event> = jobs
        .iter()
        .enumerate()
        .map(|(index, job)| Event {
            time: job.arrival as u64,
            kind: EventKind::Arrival,
            index,
        })
        .collect();

    let mut states: Vec<State> = jobs
        .iter()
        .map(|job| State {
            burst_index: 0,
            remaining: job.bursts[0],
            level: job.level,
            completion: None,
        })
        .collect();

    let mut queues: Vec<std::collections::VecDeque<usize>> = (0..quanta.len())
        .map(|_| std::collections::VecDeque::new())
        .collect();
    let mut runs = Vec::new();
    let mut idle: Vec<(u64, u64)> = Vec::new();
    let mut time = 0u64;
    let mut last_id: Option<u32> = None;
    let mut completed = 0usize;

    while completed < n {
        events.sort_by_key(|event| {
            let kind_order = match event.kind {
                EventKind::Arrival => 0usize,
                EventKind::Wake { .. } => 1usize,
            };
            (event.time, kind_order, event.index)
        });

        let mut admitted = Vec::new();
        while events.first().is_some_and(|event| event.time <= time) {
            admitted.push(events.remove(0));
        }
        for event in admitted {
            match event.kind {
                EventKind::Arrival => {
                    states[event.index].level = jobs[event.index].level;
                    queues[jobs[event.index].level].push_back(event.index);
                }
                EventKind::Wake { level } => {
                    states[event.index].level = level;
                    queues[level].push_back(event.index);
                }
            }
        }

        let Some(level) = queues.iter().position(|queue| !queue.is_empty()) else {
            if let Some(next_time) = events.first().map(|event| event.time) {
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
                    last_id = None;
                }
                continue;
            }
            break;
        };

        let index = queues[level].pop_front().unwrap();
        let id = jobs[index].id;

        if last_id.is_some_and(|previous| previous != id) {
            time += switch_cost as u64;
        }

        events.sort_by_key(|event| {
            let kind_order = match event.kind {
                EventKind::Arrival => 0usize,
                EventKind::Wake { .. } => 1usize,
            };
            (event.time, kind_order, event.index)
        });
        let mut admitted = Vec::new();
        while events.first().is_some_and(|event| event.time <= time) {
            admitted.push(events.remove(0));
        }
        for event in admitted {
            match event.kind {
                EventKind::Arrival => {
                    states[event.index].level = jobs[event.index].level;
                    queues[jobs[event.index].level].push_back(event.index);
                }
                EventKind::Wake { level } => {
                    states[event.index].level = level;
                    queues[level].push_back(event.index);
                }
            }
        }

        let run_start = time;
        let consumed = states[index].remaining.min(quanta[level]);
        time += consumed as u64;
        states[index].remaining -= consumed;
        let burst_finished = states[index].remaining == 0;
        let job_finished =
            burst_finished && states[index].burst_index + 1 == jobs[index].bursts.len();

        runs.push(Run {
            id,
            start: run_start,
            end: time,
            finished: job_finished,
            level,
        });

        if burst_finished && !job_finished {
            let io_position = states[index].burst_index + 1;
            let io_delay = jobs[index].bursts[io_position] as u64;
            let next_cpu = io_position + 1;
            states[index].burst_index = next_cpu;
            states[index].remaining = jobs[index].bursts[next_cpu];
            let wake_level = level.saturating_sub(1);
            events.push(Event {
                time: time + io_delay,
                kind: EventKind::Wake { level: wake_level },
                index,
            });
        }

        events.sort_by_key(|event| {
            let kind_order = match event.kind {
                EventKind::Arrival => 0usize,
                EventKind::Wake { .. } => 1usize,
            };
            (event.time, kind_order, event.index)
        });
        let mut admitted = Vec::new();
        while events.first().is_some_and(|event| event.time <= time) {
            admitted.push(events.remove(0));
        }
        for event in admitted {
            match event.kind {
                EventKind::Arrival => {
                    states[event.index].level = jobs[event.index].level;
                    queues[jobs[event.index].level].push_back(event.index);
                }
                EventKind::Wake { level } => {
                    states[event.index].level = level;
                    queues[level].push_back(event.index);
                }
            }
        }

        if job_finished {
            states[index].completion = Some(time);
            completed += 1;
        } else if !burst_finished {
            let demoted = (level + 1).min(quanta.len() - 1);
            states[index].level = demoted;
            queues[demoted].push_back(index);
        }

        last_id = Some(id);
    }

    let mut stats = Vec::with_capacity(n);
    let mut total_waiting = 0u64;
    let mut total_turnaround = 0u64;

    for (index, job) in jobs.iter().enumerate() {
        let completion = states[index].completion.unwrap_or(time);
        let turnaround = completion - job.arrival as u64;
        let cpu_total: u64 = job
            .bursts
            .iter()
            .step_by(2)
            .map(|&value| value as u64)
            .sum();
        let io_total: u64 = job
            .bursts
            .iter()
            .skip(1)
            .step_by(2)
            .map(|&value| value as u64)
            .sum();
        let waiting = turnaround - cpu_total - io_total;
        total_waiting += waiting;
        total_turnaround += turnaround;
        stats.push(Stat {
            id: job.id,
            completion,
            turnaround,
            waiting,
        });
    }

    let count = n as u64;
    let mean_waiting_x100 = (total_waiting * 100 + count / 2) / count;
    let mean_turnaround_x100 = (total_turnaround * 100 + count / 2) / count;

    Ok(Schedule {
        runs,
        idle,
        stats,
        mean_waiting_x100,
        mean_turnaround_x100,
        end: time,
    })
}