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
struct Waiting {
    customer: Customer,
    started: bool,
    pause_time: Option<u64>,
    remaining: u64,
}

struct Running {
    customer: Customer,
    remaining: u64,
    work_at: u64,
    finish: u64,
}

fn waiter_cmp(a: &Waiting, b: &Waiting) -> std::cmp::Ordering {
    a.customer
        .priority
        .cmp(&b.customer.priority)
        .then_with(|| b.started.cmp(&a.started))
        .then_with(|| match (a.started, b.started) {
            (true, true) => a
                .pause_time
                .unwrap_or(0)
                .cmp(&b.pause_time.unwrap_or(0))
                .then_with(|| a.customer.id.cmp(&b.customer.id)),
            (false, false) => a
                .customer
                .arrival
                .cmp(&b.customer.arrival)
                .then_with(|| b.customer.id.cmp(&a.customer.id)),
            _ => std::cmp::Ordering::Equal,
        })
}

pub fn simulate(customers: &[Customer], switch_cost: u64) -> Result<Vec<Event>, Error> {
    use std::collections::HashSet;

    let mut seen = HashSet::new();
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

    if customers.is_empty() {
        return Ok(Vec::new());
    }

    let mut arrivals = customers.to_vec();
    arrivals.sort_by(|a, b| {
        a.arrival
            .cmp(&b.arrival)
            .then_with(|| a.priority.cmp(&b.priority))
            .then_with(|| b.id.cmp(&a.id))
    });

    let mut events = Vec::new();
    let mut waiting: Vec<Waiting> = Vec::new();
    let mut running: Option<Running> = None;
    let mut next_arrival = 0usize;
    let mut now = 0u64;
    let mut last_dispatched: Option<u32> = None;
    let mut finished_count = 0usize;

    while finished_count < customers.len() {
        let next_time = {
            let mut t = arrivals.get(next_arrival).map(|c| c.arrival);
            if let Some(r) = &running {
                t = Some(t.map_or(r.finish, |x| x.min(r.finish)));
                if r.work_at > now {
                    t = Some(t.map_or(r.work_at, |x| x.min(r.work_at)));
                }
            }
            t.expect("unfinished simulation must have a pending event")
        };
        now = next_time;

        if running.as_ref().is_some_and(|r| r.finish == now) {
            let r = running.take().unwrap();
            events.push(Event {
                time: now,
                customer: r.customer.id,
                kind: Kind::Finish,
            });
            finished_count += 1;
        }

        while next_arrival < arrivals.len() && arrivals[next_arrival].arrival == now {
            let customer = arrivals[next_arrival].clone();
            events.push(Event {
                time: now,
                customer: customer.id,
                kind: Kind::Arrive,
            });
            waiting.push(Waiting {
                remaining: customer.service,
                customer,
                started: false,
                pause_time: None,
            });
            next_arrival += 1;
        }

        if let Some(r) = &running {
            if now >= r.work_at {
                if let Some(best_index) = waiting
                    .iter()
                    .enumerate()
                    .min_by(|(_, a), (_, b)| waiter_cmp(a, b))
                    .map(|(i, _)| i)
                {
                    if waiting[best_index].customer.priority < r.customer.priority {
                        let by = waiting[best_index].customer.id;
                        let r = running.take().unwrap();
                        let remaining = r.finish - now;
                        events.push(Event {
                            time: now,
                            customer: r.customer.id,
                            kind: Kind::Preempt { remaining, by },
                        });
                        waiting.push(Waiting {
                            customer: r.customer,
                            started: true,
                            pause_time: Some(now),
                            remaining,
                        });
                    }
                }
            }
        }

        while running.is_none() && !waiting.is_empty() {
            let best_index = waiting
                .iter()
                .enumerate()
                .min_by(|(_, a), (_, b)| waiter_cmp(a, b))
                .map(|(i, _)| i)
                .unwrap();
            let w = waiting.remove(best_index);

            let cost = if last_dispatched.is_some_and(|id| id != w.customer.id) {
                switch_cost
            } else {
                0
            };
            let work_at = now.checked_add(cost).ok_or(Error::TimeOverflow {
                customer: w.customer.id,
                time: now,
            })?;
            let finish = work_at.checked_add(w.remaining).ok_or(Error::TimeOverflow {
                customer: w.customer.id,
                time: now,
            })?;

            last_dispatched = Some(w.customer.id);
            let kind = if w.started {
                Kind::Resume {
                    remaining: w.remaining,
                    work_at,
                    finish,
                }
            } else {
                Kind::Start {
                    remaining: w.remaining,
                    work_at,
                    finish,
                }
            };
            events.push(Event {
                time: now,
                customer: w.customer.id,
                kind,
            });

            if finish == now {
                events.push(Event {
                    time: now,
                    customer: w.customer.id,
                    kind: Kind::Finish,
                });
                finished_count += 1;
            } else {
                running = Some(Running {
                    customer: w.customer,
                    remaining: w.remaining,
                    work_at,
                    finish,
                });
            }
        }
    }

    Ok(events)
}