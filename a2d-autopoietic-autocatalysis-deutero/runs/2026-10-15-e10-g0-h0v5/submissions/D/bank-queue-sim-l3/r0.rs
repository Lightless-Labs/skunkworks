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
struct Waiting {
    customer: Customer,
    remaining: u64,
    started: bool,
    paused_at: u64,
}

#[derive(Clone)]
struct Running {
    customer: Customer,
    remaining: u64,
    work_at: u64,
    finish: u64,
}

fn waiting_cmp(a: &Waiting, b: &Waiting) -> std::cmp::Ordering {
    use std::cmp::Ordering;

    a.customer
        .priority
        .cmp(&b.customer.priority)
        .then_with(|| b.started.cmp(&a.started))
        .then_with(|| {
            if a.started {
                a.paused_at
                    .cmp(&b.paused_at)
                    .then_with(|| a.customer.id.cmp(&b.customer.id))
            } else {
                a.customer
                    .arrival
                    .cmp(&b.customer.arrival)
                    .then_with(|| b.customer.id.cmp(&a.customer.id))
            }
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

    let mut arrivals: Vec<Customer> = customers.to_vec();
    arrivals.sort_by(|a, b| {
        a.arrival
            .cmp(&b.arrival)
            .then_with(|| a.priority.cmp(&b.priority))
            .then_with(|| b.id.cmp(&a.id))
    });

    let mut trace = Vec::new();
    let mut waiting: Vec<Waiting> = Vec::new();
    let mut running: Option<Running> = None;
    let mut arrival_index = 0;
    let mut finished_count = 0;
    let mut last_dispatched: Option<u32> = None;
    let mut now = arrivals[0].arrival;

    while finished_count < customers.len() {
        // Scheduled finishes happen before arrivals at the same time.
        if running.as_ref().is_some_and(|r| r.finish == now) {
            let r = running.take().unwrap();
            trace.push(Event {
                time: now,
                customer: r.customer.id,
                kind: Kind::Finish,
            });
            finished_count += 1;
        }

        // Emit and enqueue the full arrival cohort.
        while arrival_index < arrivals.len() && arrivals[arrival_index].arrival == now {
            let customer = arrivals[arrival_index].clone();
            trace.push(Event {
                time: now,
                customer: customer.id,
                kind: Kind::Arrive,
            });
            waiting.push(Waiting {
                remaining: customer.service,
                customer,
                started: false,
                paused_at: 0,
            });
            arrival_index += 1;
        }

        // At work_at, protection ends and preemption is reconsidered.
        if let Some(active) = running.as_ref() {
            if now >= active.work_at && now < active.finish && !waiting.is_empty() {
                let best_index = waiting
                    .iter()
                    .enumerate()
                    .min_by(|(_, a), (_, b)| waiting_cmp(a, b))
                    .map(|(i, _)| i)
                    .unwrap();

                if waiting[best_index].customer.priority < active.customer.priority {
                    let best_id = waiting[best_index].customer.id;
                    let active = running.take().unwrap();
                    let remaining = active.finish - now;
                    trace.push(Event {
                        time: now,
                        customer: active.customer.id,
                        kind: Kind::Preempt {
                            remaining,
                            by: best_id,
                        },
                    });
                    waiting.push(Waiting {
                        customer: active.customer,
                        remaining,
                        started: true,
                        paused_at: now,
                    });
                }
            }
        }

        // Dispatch repeatedly at this tick, including zero-duration work.
        while running.is_none() && !waiting.is_empty() {
            let best_index = waiting
                .iter()
                .enumerate()
                .min_by(|(_, a), (_, b)| waiting_cmp(a, b))
                .map(|(i, _)| i)
                .unwrap();
            let mut selected = waiting.remove(best_index);
            let customer_id = selected.customer.id;
            let cost = if last_dispatched.is_some_and(|id| id != customer_id) {
                switch_cost
            } else {
                0
            };

            let work_at = now.checked_add(cost).ok_or(Error::TimeOverflow {
                customer: customer_id,
                time: now,
            })?;
            let finish = work_at
                .checked_add(selected.remaining)
                .ok_or(Error::TimeOverflow {
                    customer: customer_id,
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
                customer: customer_id,
                kind,
            });

            selected.started = true;
            last_dispatched = Some(customer_id);
            running = Some(Running {
                customer: selected.customer,
                remaining: selected.remaining,
                work_at,
                finish,
            });

            if finish == now {
                let completed = running.take().unwrap();
                trace.push(Event {
                    time: now,
                    customer: completed.customer.id,
                    kind: Kind::Finish,
                });
                finished_count += 1;
            }
        }

        if finished_count == customers.len() {
            break;
        }

        let mut next = arrivals.get(arrival_index).map(|c| c.arrival);
        if let Some(active) = running.as_ref() {
            if active.finish > now {
                next = Some(next.map_or(active.finish, |t| t.min(active.finish)));
            }
            if active.work_at > now {
                next = Some(next.map_or(active.work_at, |t| t.min(active.work_at)));
            }
        }
        if let Some(next_time) = next {
            now = next_time;
        } else {
            break;
        }
    }

    Ok(trace)
}