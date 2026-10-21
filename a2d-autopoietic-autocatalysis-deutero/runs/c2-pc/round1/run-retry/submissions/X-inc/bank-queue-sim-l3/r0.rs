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
    customer: Customer,
    remaining: u64,
    started: bool,
    pause_time: Option<u64>,
}

#[derive(Clone)]
struct Running {
    job: Job,
    work_at: u64,
    finish: u64,
}

fn job_precedes(a: &Job, b: &Job) -> bool {
    if a.customer.priority != b.customer.priority {
        return a.customer.priority < b.customer.priority;
    }

    match (a.pause_time, b.pause_time) {
        (Some(pa), Some(pb)) if pa != pb => pa < pb,
        (Some(_), None) => true,
        (None, Some(_)) => false,
        (Some(_), Some(_)) => a.customer.id < b.customer.id,
        (None, None) => {
            if a.customer.arrival != b.customer.arrival {
                a.customer.arrival < b.customer.arrival
            } else {
                a.customer.id > b.customer.id
            }
        }
    }
}

pub fn simulate(customers: &[Customer], switch_cost: u64) -> Result<Vec<Event>, Error> {
    let mut ids = std::collections::HashSet::new();
    for (index, customer) in customers.iter().enumerate() {
        if customer.id == 0 {
            return Err(Error::ZeroId { index });
        }
        if !ids.insert(customer.id) {
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

    let mut arrivals: Vec<Customer> = customers.to_vec();
    arrivals.sort_by(|a, b| {
        a.arrival
            .cmp(&b.arrival)
            .then_with(|| a.priority.cmp(&b.priority))
            .then_with(|| b.id.cmp(&a.id))
    });

    let mut events = Vec::new();
    let mut waiting: Vec<Job> = Vec::new();
    let mut running: Option<Running> = None;
    let mut next_arrival = 0usize;
    let mut last_dispatched: Option<u32> = None;
    let mut now = 0u64;

    while next_arrival < arrivals.len() || running.is_some() || !waiting.is_empty() {
        let mut next_time: Option<u64> = None;
        if next_arrival < arrivals.len() {
            next_time = Some(arrivals[next_arrival].arrival);
        }
        if let Some(active) = &running {
            for t in [active.finish, active.work_at] {
                if t > now && next_time.map_or(true, |n| t < n) {
                    next_time = Some(t);
                }
            }
        }
        if let Some(t) = next_time {
            now = t;
        }

        if running.as_ref().is_some_and(|active| active.finish == now) {
            let active = running.take().unwrap();
            events.push(Event {
                time: now,
                customer: active.job.customer.id,
                kind: Kind::Finish,
            });
        }

        while next_arrival < arrivals.len() && arrivals[next_arrival].arrival == now {
            let customer = arrivals[next_arrival].clone();
            events.push(Event {
                time: now,
                customer: customer.id,
                kind: Kind::Arrive,
            });
            waiting.push(Job {
                remaining: customer.service,
                customer,
                started: false,
                pause_time: None,
            });
            next_arrival += 1;
        }

        if let Some(active) = &running {
            if now >= active.work_at {
                let best_index = waiting
                    .iter()
                    .enumerate()
                    .min_by(|(_, a), (_, b)| {
                        if job_precedes(a, b) {
                            std::cmp::Ordering::Less
                        } else if job_precedes(b, a) {
                            std::cmp::Ordering::Greater
                        } else {
                            std::cmp::Ordering::Equal
                        }
                    })
                    .map(|(i, _)| i);

                if let Some(index) = best_index {
                    if waiting[index].customer.priority < active.job.customer.priority {
                        let by = waiting[index].customer.id;
                        let mut active = running.take().unwrap();
                        let remaining = active.finish - now;
                        events.push(Event {
                            time: now,
                            customer: active.job.customer.id,
                            kind: Kind::Preempt { remaining, by },
                        });
                        active.job.remaining = remaining;
                        active.job.pause_time = Some(now);
                        waiting.push(active.job);
                    }
                }
            }
        }

        while running.is_none() && !waiting.is_empty() {
            let best_index = waiting
                .iter()
                .enumerate()
                .min_by(|(_, a), (_, b)| {
                    if job_precedes(a, b) {
                        std::cmp::Ordering::Less
                    } else if job_precedes(b, a) {
                        std::cmp::Ordering::Greater
                    } else {
                        std::cmp::Ordering::Equal
                    }
                })
                .map(|(i, _)| i)
                .unwrap();

            let mut job = waiting.remove(best_index);
            let cost = if last_dispatched.map_or(false, |id| id != job.customer.id) {
                switch_cost
            } else {
                0
            };

            let work_at = now.checked_add(cost).ok_or(Error::TimeOverflow {
                customer: job.customer.id,
                time: now,
            })?;
            let finish = work_at.checked_add(job.remaining).ok_or(Error::TimeOverflow {
                customer: job.customer.id,
                time: now,
            })?;

            let kind = if job.started {
                Kind::Resume {
                    remaining: job.remaining,
                    work_at,
                    finish,
                }
            } else {
                Kind::Start {
                    remaining: job.remaining,
                    work_at,
                    finish,
                }
            };
            events.push(Event {
                time: now,
                customer: job.customer.id,
                kind,
            });
            job.started = true;
            job.pause_time = None;
            last_dispatched = Some(job.customer.id);

            if finish == now {
                events.push(Event {
                    time: now,
                    customer: job.customer.id,
                    kind: Kind::Finish,
                });
            } else {
                running = Some(Running {
                    job,
                    work_at,
                    finish,
                });
            }
        }
    }

    Ok(events)
}