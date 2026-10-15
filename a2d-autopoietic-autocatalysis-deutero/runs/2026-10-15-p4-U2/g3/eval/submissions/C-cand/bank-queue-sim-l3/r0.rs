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
    remaining: u64,
    paused_at: Option<u64>,
    started: bool,
}

struct Running {
    customer: Customer,
    work_at: u64,
    finish: u64,
}

fn waiting_order(a: &Waiting, b: &Waiting) -> std::cmp::Ordering {
    a.customer
        .priority
        .cmp(&b.customer.priority)
        .then_with(|| match (a.paused_at, b.paused_at) {
            (Some(a_time), Some(b_time)) => a_time
                .cmp(&b_time)
                .then_with(|| a.customer.id.cmp(&b.customer.id)),
            (Some(_), None) => std::cmp::Ordering::Less,
            (None, Some(_)) => std::cmp::Ordering::Greater,
            (None, None) => a
                .customer
                .arrival
                .cmp(&b.customer.arrival)
                .then_with(|| b.customer.id.cmp(&a.customer.id)),
        })
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

    let mut arrivals = customers.to_vec();
    arrivals.sort_by(|a, b| {
        a.arrival
            .cmp(&b.arrival)
            .then_with(|| a.priority.cmp(&b.priority))
            .then_with(|| b.id.cmp(&a.id))
    });

    let mut trace = Vec::new();
    let mut waiting: Vec<Waiting> = Vec::new();
    let mut running: Option<Running> = None;
    let mut next_arrival = 0usize;
    let mut last_dispatched: Option<u32> = None;
    let mut now = 0u64;

    while next_arrival < arrivals.len() || running.is_some() || !waiting.is_empty() {
        let mut next_time = arrivals.get(next_arrival).map(|c| c.arrival);

        if let Some(active) = running.as_ref() {
            for t in [active.finish, active.work_at] {
                if t > now && next_time.map_or(true, |current| t < current) {
                    next_time = Some(t);
                }
            }
        }

        let Some(time) = next_time else {
            break;
        };
        now = time;

        if running.as_ref().is_some_and(|active| active.finish == now) {
            let active = running.take().unwrap();
            trace.push(Event {
                time: now,
                customer: active.customer.id,
                kind: Kind::Finish,
            });
        }

        while next_arrival < arrivals.len() && arrivals[next_arrival].arrival == now {
            let customer = arrivals[next_arrival].clone();
            trace.push(Event {
                time: now,
                customer: customer.id,
                kind: Kind::Arrive,
            });
            waiting.push(Waiting {
                remaining: customer.service,
                customer,
                paused_at: None,
                started: false,
            });
            next_arrival += 1;
        }

        if let Some(active) = running.as_ref() {
            if now >= active.work_at && now < active.finish {
                waiting.sort_by(waiting_order);
                if waiting.first().is_some_and(|waiter| {
                    waiter.customer.priority < active.customer.priority
                }) {
                    let active = running.take().unwrap();
                    let remaining = active.finish - now;
                    let by = waiting[0].customer.id;
                    trace.push(Event {
                        time: now,
                        customer: active.customer.id,
                        kind: Kind::Preempt { remaining, by },
                    });
                    waiting.push(Waiting {
                        customer: active.customer,
                        remaining,
                        paused_at: Some(now),
                        started: true,
                    });
                }
            }
        }

        while running.is_none() && !waiting.is_empty() {
            waiting.sort_by(waiting_order);
            let selected = waiting.remove(0);
            let id = selected.customer.id;
            let cost = if last_dispatched.is_some_and(|previous| previous != id) {
                switch_cost
            } else {
                0
            };

            let work_at = now.checked_add(cost).ok_or(Error::TimeOverflow {
                customer: id,
                time: now,
            })?;
            let finish = work_at
                .checked_add(selected.remaining)
                .ok_or(Error::TimeOverflow {
                    customer: id,
                    time: now,
                })?;

            let kind = if selected.started {
                Kind::Resume {
                    remaining: selected.remaining,
                    work_at,
                    finish,
                }
            } else {
                Kind::Start {
                    remaining: selected.remaining,
                    work_at,
                    finish,
                }
            };
            trace.push(Event {
                time: now,
                customer: id,
                kind,
            });
            last_dispatched = Some(id);

            if finish == now {
                trace.push(Event {
                    time: now,
                    customer: id,
                    kind: Kind::Finish,
                });
            } else {
                running = Some(Running {
                    customer: selected.customer,
                    work_at,
                    finish,
                });
            }
        }
    }

    Ok(trace)
}

fn main() {}