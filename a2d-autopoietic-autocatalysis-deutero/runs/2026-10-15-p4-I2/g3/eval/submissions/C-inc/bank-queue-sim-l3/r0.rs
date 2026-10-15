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
    ZeroId {
        index: usize,
    },
    DuplicateId {
        index: usize,
        id: u32,
    },
    BadPriority {
        index: usize,
        value: u8,
    },
    TimeOverflow {
        customer: u32,
        time: u64,
    },
}

#[derive(Clone)]
struct Job {
    customer: Customer,
    started: bool,
    remaining: u64,
    pause_time: Option<u64>,
}

struct Active {
    job: Job,
    work_at: u64,
    finish: u64,
}

fn waiter_order(a: &Job, b: &Job) -> std::cmp::Ordering {
    a.customer
        .priority
        .cmp(&b.customer.priority)
        .then_with(|| match (a.pause_time, b.pause_time) {
            (Some(pa), Some(pb)) => pa.cmp(&pb).then_with(|| a.customer.id.cmp(&b.customer.id)),
            (Some(_), None) => std::cmp::Ordering::Less,
            (None, Some(_)) => std::cmp::Ordering::Greater,
            (None, None) => a
                .customer
                .arrival
                .cmp(&b.customer.arrival)
                .then_with(|| b.customer.id.cmp(&a.customer.id)),
        })
}

fn take_best(waiting: &mut Vec<Job>) -> Option<Job> {
    let index = waiting
        .iter()
        .enumerate()
        .min_by(|(_, a), (_, b)| waiter_order(a, b))
        .map(|(i, _)| i)?;
    Some(waiting.remove(index))
}

pub fn simulate(customers: &[Customer], switch_cost: u64) -> Result<Vec<Event>, Error> {
    use std::collections::HashSet;

    let mut ids = HashSet::new();
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
    let mut active: Option<Active> = None;
    let mut next_arrival = 0usize;
    let mut last_dispatched: Option<u32> = None;

    while next_arrival < arrivals.len() || active.is_some() || !waiting.is_empty() {
        let mut now = u64::MAX;
        if let Some(customer) = arrivals.get(next_arrival) {
            now = now.min(customer.arrival);
        }
        if let Some(a) = &active {
            now = now.min(a.finish);
            if a.work_at > now || (a.work_at == now && now < a.finish) {
                now = now.min(a.work_at);
            }
        }

        if let Some(a) = active.as_ref() {
            if a.finish == now {
                let id = a.job.customer.id;
                events.push(Event {
                    time: now,
                    customer: id,
                    kind: Kind::Finish,
                });
                active = None;
            }
        }

        let cohort_start = next_arrival;
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
        let _ = cohort_start;

        if let Some(a) = active.as_ref() {
            if now >= a.work_at {
                let best_priority = waiting
                    .iter()
                    .map(|job| job.customer.priority)
                    .min();
                if best_priority.is_some_and(|p| p < a.job.customer.priority) {
                    let mut a = active.take().unwrap();
                    let remaining = a.finish - now;
                    let by = waiting
                        .iter()
                        .filter(|job| Some(job.customer.priority) == best_priority)
                        .min_by(|x, y| waiter_order(x, y))
                        .unwrap()
                        .customer
                        .id;
                    events.push(Event {
                        time: now,
                        customer: a.job.customer.id,
                        kind: Kind::Preempt { remaining, by },
                    });
                    a.job.remaining = remaining;
                    a.job.pause_time = Some(now);
                    waiting.push(a.job);
                }
            }
        }

        if active.is_none() {
            while let Some(mut job) = take_best(&mut waiting) {
                let cost = if last_dispatched == Some(job.customer.id) {
                    0
                } else if last_dispatched.is_none() {
                    0
                } else {
                    switch_cost
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
                job.started = true;
                job.pause_time = None;
                last_dispatched = Some(job.customer.id);
                events.push(Event {
                    time: now,
                    customer: job.customer.id,
                    kind,
                });

                if finish == now {
                    events.push(Event {
                        time: now,
                        customer: job.customer.id,
                        kind: Kind::Finish,
                    });
                    continue;
                }

                active = Some(Active {
                    job,
                    work_at,
                    finish,
                });
                break;
            }
        }
    }

    Ok(events)
}

fn main() {}