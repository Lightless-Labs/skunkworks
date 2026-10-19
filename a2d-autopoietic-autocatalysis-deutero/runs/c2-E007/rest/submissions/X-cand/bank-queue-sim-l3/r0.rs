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
    pause_time: Option<u64>,
}

struct Active {
    customer: Customer,
    remaining: u64,
    work_at: u64,
    finish: u64,
}

fn waiting_precedes(a: &Waiting, b: &Waiting) -> bool {
    if a.customer.priority != b.customer.priority {
        return a.customer.priority < b.customer.priority;
    }

    match (a.pause_time, b.pause_time) {
        (Some(a_time), Some(b_time)) => {
            a_time < b_time
                || (a_time == b_time && a.customer.id < b.customer.id)
        }
        (Some(_), None) => true,
        (None, Some(_)) => false,
        (None, None) => {
            a.customer.arrival < b.customer.arrival
                || (a.customer.arrival == b.customer.arrival
                    && a.customer.id > b.customer.id)
        }
    }
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
    let mut waiting: Vec<Waiting> = Vec::new();
    let mut active: Option<Active> = None;
    let mut next_arrival = 0usize;
    let mut last_dispatched: Option<u32> = None;
    let mut finished_count = 0usize;
    let mut now = arrivals[0].arrival;

    while finished_count < customers.len() {
        if let Some(running) = active.as_ref() {
            if running.finish == now {
                let completed = active.take().unwrap();
                events.push(Event {
                    time: now,
                    customer: completed.customer.id,
                    kind: Kind::Finish,
                });
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
            waiting.push(Waiting {
                remaining: customer.service,
                customer,
                started: false,
                pause_time: None,
            });
            next_arrival += 1;
        }

        if let Some(running) = active.as_ref() {
            if now >= running.work_at {
                let best = waiting
                    .iter()
                    .enumerate()
                    .min_by(|(_, a), (_, b)| {
                        if waiting_precedes(a, b) {
                            std::cmp::Ordering::Less
                        } else if waiting_precedes(b, a) {
                            std::cmp::Ordering::Greater
                        } else {
                            std::cmp::Ordering::Equal
                        }
                    })
                    .map(|(index, _)| index);

                if let Some(index) = best {
                    if waiting[index].customer.priority < running.customer.priority {
                        let by = waiting[index].customer.id;
                        let running = active.take().unwrap();
                        let remaining = running.finish - now;
                        events.push(Event {
                            time: now,
                            customer: running.customer.id,
                            kind: Kind::Preempt { remaining, by },
                        });
                        waiting.push(Waiting {
                            customer: running.customer,
                            remaining,
                            started: true,
                            pause_time: Some(now),
                        });
                    }
                }
            }
        }

        if active.is_none() {
            while !waiting.is_empty() {
                let best_index = (0..waiting.len())
                    .min_by(|&a, &b| {
                        if waiting_precedes(&waiting[a], &waiting[b]) {
                            std::cmp::Ordering::Less
                        } else if waiting_precedes(&waiting[b], &waiting[a]) {
                            std::cmp::Ordering::Greater
                        } else {
                            std::cmp::Ordering::Equal
                        }
                    })
                    .unwrap();
                let chosen = waiting.remove(best_index);
                let cost = if last_dispatched == Some(chosen.customer.id) {
                    0
                } else if last_dispatched.is_none() {
                    0
                } else {
                    switch_cost
                };
                let work_at = now.checked_add(cost).ok_or(Error::TimeOverflow {
                    customer: chosen.customer.id,
                    time: now,
                })?;
                let finish = work_at.checked_add(chosen.remaining).ok_or(
                    Error::TimeOverflow {
                        customer: chosen.customer.id,
                        time: now,
                    },
                )?;

                let kind = if chosen.started {
                    Kind::Resume {
                        remaining: chosen.remaining,
                        work_at,
                        finish,
                    }
                } else {
                    Kind::Start {
                        remaining: chosen.remaining,
                        work_at,
                        finish,
                    }
                };
                events.push(Event {
                    time: now,
                    customer: chosen.customer.id,
                    kind,
                });
                last_dispatched = Some(chosen.customer.id);
                active = Some(Active {
                    customer: chosen.customer,
                    remaining: chosen.remaining,
                    work_at,
                    finish,
                });

                if finish == now {
                    let completed = active.take().unwrap();
                    events.push(Event {
                        time: now,
                        customer: completed.customer.id,
                        kind: Kind::Finish,
                    });
                    finished_count += 1;
                    continue;
                }
                break;
            }
        }

        if finished_count == customers.len() {
            break;
        }

        let mut next_time: Option<u64> = None;
        if next_arrival < arrivals.len() {
            next_time = Some(arrivals[next_arrival].arrival);
        }
        if let Some(running) = active.as_ref() {
            for t in [Some(running.finish), Some(running.work_at)] {
                if let Some(t) = t {
                    if t > now && next_time.map_or(true, |current| t < current) {
                        next_time = Some(t);
                    }
                }
            }
        }

        if let Some(t) = next_time {
            now = t;
        } else if !waiting.is_empty() {
            // An idle server with waiters is dispatched above; this is only a
            // defensive guard against an impossible stalled state.
            now = now.saturating_add(1);
        } else {
            break;
        }
    }

    Ok(events)
}