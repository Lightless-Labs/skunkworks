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
    paused_at: Option<u64>,
}

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
        .then_with(|| match (a.paused_at, b.paused_at) {
            (Some(at), Some(bt)) => at.cmp(&bt),
            (Some(_), None) => Ordering::Less,
            (None, Some(_)) => Ordering::Greater,
            (None, None) => a
                .customer
                .arrival
                .cmp(&b.customer.arrival)
                .then_with(|| b.customer.id.cmp(&a.customer.id)),
        })
        .then_with(|| a.customer.id.cmp(&b.customer.id))
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

    let mut events = Vec::new();
    let mut waiting: Vec<Waiting> = Vec::new();
    let mut running: Option<Running> = None;
    let mut arrival_index = 0usize;
    let mut finished_count = 0usize;
    let mut last_dispatched: Option<u32> = None;
    let mut time = arrivals[0].arrival;

    while finished_count < customers.len() {
        // Scheduled completion precedes every other event at this tick.
        if running.as_ref().is_some_and(|r| r.finish == time) {
            let completed = running.take().unwrap();
            events.push(Event {
                time,
                customer: completed.customer.id,
                kind: Kind::Finish,
            });
            finished_count += 1;
        }

        // Admit the entire arrival cohort before making scheduling decisions.
        while arrival_index < arrivals.len() && arrivals[arrival_index].arrival == time {
            let customer = arrivals[arrival_index].clone();
            events.push(Event {
                time,
                customer: customer.id,
                kind: Kind::Arrive,
            });
            waiting.push(Waiting {
                remaining: customer.service,
                customer,
                started: false,
                paused_at: None,
            });
            arrival_index += 1;
        }

        // Protection ends at work_at; a running customer can then be preempted.
        if let Some(active) = running.as_ref() {
            if time >= active.work_at && time < active.finish {
                if let Some(best_index) = best_waiter_index(&waiting) {
                    if waiting[best_index].customer.priority < active.customer.priority {
                        let best_id = waiting[best_index].customer.id;
                        let active = running.take().unwrap();
                        let remaining = active.finish - time;
                        events.push(Event {
                            time,
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
                            paused_at: Some(time),
                        });
                    }
                }
            }
        }

        // Fill an idle server. Inline completions repeat at this same timestamp.
        while running.is_none() {
            let Some(index) = best_waiter_index(&waiting) else {
                break;
            };
            let selected = waiting.remove(index);
            let id = selected.customer.id;
            let cost = if last_dispatched == Some(id) {
                0
            } else if last_dispatched.is_none() {
                0
            } else {
                switch_cost
            };
            let work_at = time.checked_add(cost).ok_or(Error::TimeOverflow {
                customer: id,
                time,
            })?;
            let finish = work_at.checked_add(selected.remaining).ok_or(Error::TimeOverflow {
                customer: id,
                time,
            })?;

            last_dispatched = Some(id);
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
            events.push(Event {
                time,
                customer: id,
                kind,
            });

            running = Some(Running {
                customer: selected.customer,
                remaining: selected.remaining,
                work_at,
                finish,
            });

            if finish == time {
                let completed = running.take().unwrap();
                events.push(Event {
                    time,
                    customer: completed.customer.id,
                    kind: Kind::Finish,
                });
                finished_count += 1;
                continue;
            }
            break;
        }

        if finished_count == customers.len() {
            break;
        }

        // Advance to the next arrival or active completion/protection boundary.
        let mut next_time: Option<u64> = if arrival_index < arrivals.len() {
            Some(arrivals[arrival_index].arrival)
        } else {
            None
        };

        if let Some(active) = &running {
            for boundary in [active.work_at, active.finish] {
                if boundary > time {
                    next_time = Some(next_time.map_or(boundary, |n| n.min(boundary)));
                }
            }
        }

        if let Some(next) = next_time {
            time = next;
        } else {
            // With no running work, no arrivals, and unfinished customers, the
            // only remaining work must be represented in the waiting queue.
            if waiting.is_empty() {
                break;
            }
            // A waiting customer should be dispatched above; this is defensive.
            time = time.saturating_add(1);
        }
    }

    Ok(events)
}

fn best_waiter_index(waiting: &[Waiting]) -> Option<usize> {
    waiting
        .iter()
        .enumerate()
        .min_by(|(_, a), (_, b)| waiting_cmp(a, b))
        .map(|(index, _)| index)
}