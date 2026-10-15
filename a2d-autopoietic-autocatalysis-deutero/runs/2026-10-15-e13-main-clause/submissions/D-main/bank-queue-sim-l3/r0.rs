#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Customer {
    pub id: u32,
    pub arrival: u64,
    pub service: u64,
    pub priority: u8,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Kind {
    Arrive,
    Start {
        remaining: u64,
        work_at: u64,
        finish: u64,
    },
    Resume {
        remaining: u64,
        work_at: u64,
        finish: u64,
    },
    Preempt {
        remaining: u64,
        by: u32,
    },
    Finish,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Event {
    pub time: u64,
    pub customer: u32,
    pub kind: Kind,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Error {
    ZeroId { index: usize },
    DuplicateId { index: usize, id: u32 },
    BadPriority { index: usize, value: u8 },
    TimeOverflow { customer: u32, time: u64 },
}

#[derive(Clone)]
struct Job {
    id: u32,
    arrival: u64,
    service: u64,
    priority: u8,
    started: bool,
    pause_time: u64,
}

struct Running {
    job: Job,
    remaining: u64,
    work_at: u64,
    finish: u64,
}

fn better_waiter(a: &Job, b: &Job) -> bool {
    if a.priority != b.priority {
        return a.priority < b.priority;
    }

    match (a.started, b.started) {
        (true, false) => true,
        (false, true) => false,
        (true, true) => (a.pause_time, a.id) < (b.pause_time, b.id),
        (false, false) => {
            a.arrival < b.arrival || (a.arrival == b.arrival && a.id > b.id)
        }
    }
}

fn best_index(waiting: &[Job]) -> Option<usize> {
    let mut best = None;
    for i in 0..waiting.len() {
        if best.map_or(true, |j| better_waiter(&waiting[i], &waiting[j])) {
            best = Some(i);
        }
    }
    best
}

pub fn simulate(customers: &[Customer], switch_cost: u64) -> Result<Vec<Event>, Error> {
    let mut seen = std::collections::HashSet::new();
    for (index, customer) in customers.iter().enumerate() {
        if customer.id == 0 {
            return Err(Error::ZeroId { index });
        }
        if !seen.insert(customer.id) {
            return Err(Error::DuplicateId {
                index,
                id: customer.id,
            });
        }
        if customer.priority > 3 {
            return Err(Error::BadPriority {
                index,
                value: customer.priority,
            });
        }
    }

    let mut jobs: Vec<Job> = customers
        .iter()
        .map(|customer| Job {
            id: customer.id,
            arrival: customer.arrival,
            service: customer.service,
            priority: customer.priority,
            started: false,
            pause_time: 0,
        })
        .collect();

    jobs.sort_by(|a, b| {
        a.arrival
            .cmp(&b.arrival)
            .then(a.priority.cmp(&b.priority))
            .then(b.id.cmp(&a.id))
    });

    let mut events = Vec::new();
    let mut waiting: Vec<Job> = Vec::new();
    let mut running: Option<Running> = None;
    let mut next_arrival = 0usize;
    let mut last_dispatched: Option<u32> = None;
    let mut now = 0u64;
    let mut finished_count = 0usize;

    while finished_count < jobs.len() {
        let next_arrival_time = jobs.get(next_arrival).map(|job| job.arrival);
        let next_finish = running.as_ref().map(|r| r.finish);
        let next_work_at = running
            .as_ref()
            .map(|r| r.work_at)
            .filter(|&time| time > now);

        let Some(tick) = [next_arrival_time, next_finish, next_work_at]
            .into_iter()
            .flatten()
            .filter(|&time| time >= now)
            .min()
        else {
            break;
        };
        now = tick;

        if running.as_ref().is_some_and(|r| r.finish == now) {
            let completed = running.take().unwrap();
            events.push(Event {
                time: now,
                customer: completed.job.id,
                kind: Kind::Finish,
            });
            finished_count += 1;
        }

        while next_arrival < jobs.len() && jobs[next_arrival].arrival == now {
            let job = jobs[next_arrival].clone();
            events.push(Event {
                time: now,
                customer: job.id,
                kind: Kind::Arrive,
            });
            waiting.push(job);
            next_arrival += 1;
        }

        if let Some(r) = running.as_ref() {
            if now >= r.work_at {
                if let Some(i) = best_index(&waiting) {
                    if waiting[i].priority < r.job.priority {
                        let by = waiting[i].id;
                        let mut paused = running.take().unwrap();
                        let remaining = paused.finish - now;
                        events.push(Event {
                            time: now,
                            customer: paused.job.id,
                            kind: Kind::Preempt { remaining, by },
                        });
                        paused.job.started = true;
                        paused.job.pause_time = now;
                        paused.remaining = remaining;
                        paused.job.service = remaining;
                        waiting.push(paused.job);
                    }
                }
            }
        }

        loop {
            if running.is_some() {
                break;
            }
            let Some(i) = best_index(&waiting) else {
                break;
            };
            let job = waiting.remove(i);
            let cost = if last_dispatched.is_some_and(|id| id != job.id) {
                switch_cost
            } else {
                0
            };

            let work_at = now.checked_add(cost).ok_or(Error::TimeOverflow {
                customer: job.id,
                time: now,
            })?;
            let finish = work_at
                .checked_add(job.service)
                .ok_or(Error::TimeOverflow {
                    customer: job.id,
                    time: now,
                })?;

            let kind = if job.started {
                Kind::Resume {
                    remaining: job.service,
                    work_at,
                    finish,
                }
            } else {
                Kind::Start {
                    remaining: job.service,
                    work_at,
                    finish,
                }
            };
            events.push(Event {
                time: now,
                customer: job.id,
                kind,
            });
            last_dispatched = Some(job.id);
            running = Some(Running {
                job,
                remaining: 0,
                work_at,
                finish,
            });

            if finish == now {
                let completed = running.take().unwrap();
                events.push(Event {
                    time: now,
                    customer: completed.job.id,
                    kind: Kind::Finish,
                });
                finished_count += 1;
                continue;
            }
            break;
        }
    }

    Ok(events)
}

fn main() {}