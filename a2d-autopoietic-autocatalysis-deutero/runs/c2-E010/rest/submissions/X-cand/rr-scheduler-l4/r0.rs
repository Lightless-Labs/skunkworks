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

#[derive(Clone)]
struct Process {
    job: Job,
    burst: usize,
    remaining: u32,
    level: usize,
    completion: Option<u64>,
    cpu: u64,
    io: u64,
}

#[derive(Clone, Copy)]
struct Event {
    at: u64,
    kind: u8,
    index: usize,
    wake_level: usize,
}

fn push_idle(idle: &mut Vec<(u64, u64)>, from: u64, to: u64) {
    if from >= to {
        return;
    }
    if let Some(pair) = idle.last_mut() {
        if pair.1 == from {
            pair.1 = to;
            return;
        }
    }
    idle.push((from, to));
}

pub fn schedule(jobs: &[Job], quanta: &[u32], switch_cost: u32) -> Result<Schedule, Error> {
    if quanta.is_empty() || quanta.len() > 8 {
        return Err(Error::LevelCount);
    }
    for (level, quantum) in quanta.iter().enumerate() {
        if *quantum == 0 || *quantum > 1000 {
            return Err(Error::Quantum { level });
        }
    }
    if switch_cost > 1000 {
        return Err(Error::SwitchCost);
    }
    if jobs.len() > 64 {
        return Err(Error::TooManyJobs);
    }
    for (i, job) in jobs.iter().enumerate() {
        if jobs[..i].iter().any(|old| old.id == job.id) {
            return Err(Error::DuplicateId { index: i });
        }
        if job.arrival > 1000 {
            return Err(Error::Arrival { index: i });
        }
        if job.level >= quanta.len() {
            return Err(Error::InitialLevel { index: i });
        }
        if job.bursts.len() == 0 || job.bursts.len() > 31 || job.bursts.len() % 2 == 0 {
            return Err(Error::BurstCount { index: i });
        }
        for (pos, value) in job.bursts.iter().enumerate() {
            if (pos % 2 == 0 && *value == 0) || *value > 1000 {
                return Err(Error::Burst {
                    index: i,
                    position: pos,
                });
            }
        }
    }

    let mut ps: Vec<Process> = jobs
        .iter()
        .cloned()
        .map(|job| Process {
            burst: 0,
            remaining: job.bursts[0],
            level: job.level,
            completion: None,
            cpu: job
                .bursts
                .iter()
                .step_by(2)
                .map(|&n| n as u64)
                .sum(),
            io: job
                .bursts
                .iter()
                .skip(1)
                .step_by(2)
                .map(|&n| n as u64)
                .sum(),
            job,
        })
        .collect();

    let mut queues: Vec<std::collections::VecDeque<usize>> =
        (0..quanta.len()).map(|_| std::collections::VecDeque::new()).collect();
    let mut events: Vec<Event> = jobs
        .iter()
        .enumerate()
        .map(|(i, job)| Event {
            at: job.arrival as u64,
            kind: 0,
            index: i,
            wake_level: 0,
        })
        .collect();
    let mut now = 0u64;
    let mut last: Option<u32> = None;
    let mut runs = Vec::new();
    let mut idle = Vec::new();
    let mut done = 0usize;

    while done < jobs.len() {
        events.sort_by_key(|e| (e.at, e.kind, e.index));
        let mut consumed_events = 0;
        while consumed_events < events.len() && events[consumed_events].at <= now {
            let event = events[consumed_events];
            if event.kind == 0 {
                queues[ps[event.index].level].push_back(event.index);
            } else {
                ps[event.index].level = event.wake_level;
                queues[event.wake_level].push_back(event.index);
            }
            consumed_events += 1;
        }
        events.drain(0..consumed_events);

        let Some(level) = queues.iter().position(|q| !q.is_empty()) else {
            if events.is_empty() {
                break;
            }
            let next = events[0].at;
            if next > now {
                push_idle(&mut idle, now, next);
                now = next;
                last = None;
            }
            continue;
        };

        let i = queues[level].pop_front().unwrap();
        let id = ps[i].job.id;
        if last.is_some_and(|old| old != id) {
            now += switch_cost as u64;
        }

        events.sort_by_key(|e| (e.at, e.kind, e.index));
        let mut n = 0;
        while n < events.len() && events[n].at <= now {
            let ev = events[n];
            if ev.kind == 0 {
                queues[ps[ev.index].level].push_back(ev.index);
            } else {
                ps[ev.index].level = ev.wake_level;
                queues[ev.wake_level].push_back(ev.index);
            }
            n += 1;
        }
        events.drain(0..n);

        let start = now;
        let ran = ps[i].remaining.min(quanta[level]);
        now += ran as u64;
        ps[i].remaining -= ran;
        let burst_done = ps[i].remaining == 0;
        let finished = burst_done && ps[i].burst + 1 == ps[i].job.bursts.len();
        runs.push(Run {
            id,
            start,
            end: now,
            finished,
            level,
        });

        if burst_done && !finished {
            let delay = ps[i].job.bursts[ps[i].burst + 1] as u64;
            ps[i].burst += 2;
            ps[i].remaining = ps[i].job.bursts[ps[i].burst];
            events.push(Event {
                at: now + delay,
                kind: 1,
                index: i,
                wake_level: level.saturating_sub(1),
            });
        }

        events.sort_by_key(|e| (e.at, e.kind, e.index));
        let mut n = 0;
        while n < events.len() && events[n].at <= now {
            let ev = events[n];
            if ev.kind == 0 {
                queues[ps[ev.index].level].push_back(ev.index);
            } else {
                ps[ev.index].level = ev.wake_level;
                queues[ev.wake_level].push_back(ev.index);
            }
            n += 1;
        }
        events.drain(0..n);

        if finished {
            ps[i].completion = Some(now);
            done += 1;
        } else if !burst_done {
            ps[i].level = (level + 1).min(quanta.len() - 1);
            queues[ps[i].level].push_back(i);
        }
        last = Some(id);
    }

    let stats: Vec<Stat> = ps
        .iter()
        .map(|p| {
            let completion = p.completion.unwrap_or(now);
            let turnaround = completion - p.job.arrival as u64;
            Stat {
                id: p.job.id,
                completion,
                turnaround,
                waiting: turnaround - p.cpu - p.io,
            }
        })
        .collect();
    let total_waiting: u64 = stats.iter().map(|x| x.waiting).sum();
    let total_turnaround: u64 = stats.iter().map(|x| x.turnaround).sum();
    let count = jobs.len() as u64;
    let mean_waiting_x100 = if count == 0 {
        0
    } else {
        (total_waiting * 100 + count / 2) / count
    };
    let mean_turnaround_x100 = if count == 0 {
        0
    } else {
        (total_turnaround * 100 + count / 2) / count
    };
    let end = stats.iter().map(|x| x.completion).max().unwrap_or(0);

    Ok(Schedule {
        runs,
        idle,
        stats,
        mean_waiting_x100,
        mean_turnaround_x100,
        end,
    })
}
