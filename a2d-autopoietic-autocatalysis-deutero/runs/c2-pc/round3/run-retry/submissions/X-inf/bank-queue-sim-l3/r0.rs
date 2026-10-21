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
    paused_at: Option<u64>,
}

#[derive(Clone)]
struct Running {
    job: Job,
    work_at: u64,
    finish: u64,
}

fn waiter_cmp(a: &Job, b: &Job) -> std::cmp::Ordering {
    a.customer
        .priority
        .cmp(&b.customer.priority)
        .then_with(|| match (a.paused_at, b.paused_at) {
            (Some(at_a), Some(at_b)) => at_a.cmp(&at_b),
            (Some(_), None) => std::cmp::Ordering::Less,
            (None, Some(_)) => std::cmp::Ordering::Greater,
            (None, None) => a
                .customer
                .arrival
                .cmp(&b.customer.arrival)
                .then_with(|| b.customer.id.cmp(&a.customer.id)),
        })
        .then_with(|| a.customer.id.cmp(&b.customer.id))
}

fn best_waiter_index(waiting: &[Job]) -> Option<usize> {
    waiting
        .iter()
        .enumerate()
        .min_by(|(_, a), (_, b)| waiter_cmp(a, b))
        .map(|(i, _)| i)
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

    if customers.is_empty() {
        return Ok(Vec::new());
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
    let mut last_dispatched: Option<u32> = None;
    let mut next_arrival = 0usize;
    let mut finished_count = 0usize;
    let mut now = arrivals[0].arrival;

    while finished_count < customers.len() {
        if let Some(active) = running.as_ref() {
            if active.finish == now {
                let id = active.job.customer.id;
                events.push(Event {
                    time: now,
                    customer: id,
                    kind: Kind::Finish,
                });
                running = None;
                finished_count += 1;
            }
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
                paused_at: None,
            });
            next_arrival += 1;
        }

        if let Some(active) = running.as_ref() {
            if now >= active.work_at {
                if let Some(wait_idx) = best_waiter_index(&waiting) {
                    if waiting[wait_idx].customer.priority < active.job.customer.priority {
                        let waiter_id = waiting[wait_idx].customer.id;
                        let remaining = active.finish - now;
                        let mut paused = running.take().unwrap().job;
                        paused.remaining = remaining;
                        paused.paused_at = Some(now);
                        events.push(Event {
                            time: now,
                            customer: paused.customer.id,
                            kind: Kind::Preempt {
                                remaining,
                                by: waiter_id,
                            },
                        });
                        waiting.push(paused);
                    }
                }
            }
        }

        while running.is_none() {
            let Some(index) = best_waiter_index(&waiting) else {
                break;
            };
            let mut job = waiting.remove(index);
            let id = job.customer.id;
            let cost = if last_dispatched.is_some_and(|previous| previous != id) {
                switch_cost
            } else {
                0
            };
            let work_at = now
                .checked_add(cost)
                .ok_or(Error::TimeOverflow {
                    customer: id,
                    time: now,
                })?;
            let finish = work_at
                .checked_add(job.remaining)
                .ok_or(Error::TimeOverflow {
                    customer: id,
                    time: now,
                })?;

            let remaining = job.remaining;
            let kind = if job.started {
                Kind::Resume {
                    remaining,
                    work_at,
                    finish,
                }
            } else {
                Kind::Start {
                    remaining,
                    work_at,
                    finish,
                }
            };
            job.started = true;
            job.paused_at = None;
            events.push(Event {
                time: now,
                customer: id,
                kind,
            });
            last_dispatched = Some(id);
            running = Some(Running {
                job,
                work_at,
                finish,
            });

            if finish == now {
                events.push(Event {
                    time: now,
                    customer: id,
                    kind: Kind::Finish,
                });
                running = None;
                finished_count += 1;
            }
        }

        if finished_count == customers.len() {
            break;
        }

        let mut next_time = if next_arrival < arrivals.len() {
            Some(arrivals[next_arrival].arrival)
        } else {
            None
        };
        if let Some(active) = running.as_ref() {
            for boundary in [Some(active.finish), Some(active.work_at)].into_iter().flatten() {
                if boundary > now {
                    next_time = Some(next_time.map_or(boundary, |current| current.min(boundary)));
                }
            }
        }

        match next_time {
            Some(time) => now = time,
            None => break,
        }
    }

    Ok(events)
}