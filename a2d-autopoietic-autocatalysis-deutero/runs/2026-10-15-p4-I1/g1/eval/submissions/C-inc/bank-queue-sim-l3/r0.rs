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
    index: usize,
    paused_at: Option<u64>,
    remaining: u64,
}

#[derive(Clone)]
struct Running {
    index: usize,
    remaining: u64,
    work_at: u64,
    finish: u64,
}

fn waiting_cmp(a: &Waiting, b: &Waiting, customers: &[Customer]) -> std::cmp::Ordering {
    let ca = &customers[a.index];
    let cb = &customers[b.index];

    ca.priority
        .cmp(&cb.priority)
        .then_with(|| match (a.paused_at, b.paused_at) {
            (Some(pa), Some(pb)) => pa.cmp(&pb).then_with(|| ca.id.cmp(&cb.id)),
            (Some(_), None) => std::cmp::Ordering::Less,
            (None, Some(_)) => std::cmp::Ordering::Greater,
            (None, None) => ca
                .arrival
                .cmp(&cb.arrival)
                .then_with(|| cb.id.cmp(&ca.id)),
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

    let mut arrivals: Vec<usize> = (0..customers.len()).collect();
    arrivals.sort_by(|&a, &b| {
        customers[a]
            .arrival
            .cmp(&customers[b].arrival)
            .then_with(|| customers[a].priority.cmp(&customers[b].priority))
            .then_with(|| customers[b].id.cmp(&customers[a].id))
    });

    let mut events = Vec::new();
    let mut waiting: Vec<Waiting> = Vec::new();
    let mut running: Option<Running> = None;
    let mut next_arrival = 0;
    let mut now = 0;
    let mut started = vec![false; customers.len()];
    let mut last_dispatched: Option<u32> = None;
    let mut finished_count = 0;

    while finished_count < customers.len() {
        let mut next_tick: Option<u64> = None;
        if next_arrival < arrivals.len() {
            next_tick = Some(customers[arrivals[next_arrival]].arrival);
        }
        if let Some(active) = &running {
            for tick in [active.finish, active.work_at] {
                if tick > now && next_tick.map_or(true, |n| tick < n) {
                    next_tick = Some(tick);
                }
            }
        }

        if let Some(tick) = next_tick {
            now = tick;
        } else if running.is_none() && next_arrival >= arrivals.len() {
            break;
        }

        if running.as_ref().is_some_and(|active| active.finish == now) {
            let active = running.take().unwrap();
            events.push(Event {
                time: now,
                customer: customers[active.index].id,
                kind: Kind::Finish,
            });
            finished_count += 1;
        }

        while next_arrival < arrivals.len()
            && customers[arrivals[next_arrival]].arrival == now
        {
            let index = arrivals[next_arrival];
            events.push(Event {
                time: now,
                customer: customers[index].id,
                kind: Kind::Arrive,
            });
            waiting.push(Waiting {
                index,
                paused_at: None,
                remaining: customers[index].service,
            });
            next_arrival += 1;
        }

        waiting.sort_by(|a, b| waiting_cmp(a, b, customers));

        if let Some(active) = &running {
            if now >= active.work_at {
                if let Some(best) = waiting.first() {
                    if customers[best.index].priority < customers[active.index].priority {
                        let best_id = customers[best.index].id;
                        let active = running.take().unwrap();
                        let remaining = active.finish - now;
                        events.push(Event {
                            time: now,
                            customer: customers[active.index].id,
                            kind: Kind::Preempt {
                                remaining,
                                by: best_id,
                            },
                        });
                        waiting.push(Waiting {
                            index: active.index,
                            paused_at: Some(now),
                            remaining,
                        });
                    }
                }
            }
        }

        loop {
            if running.is_some() || waiting.is_empty() {
                break;
            }

            waiting.sort_by(|a, b| waiting_cmp(a, b, customers));
            let selected = waiting.remove(0);
            let customer = &customers[selected.index];
            let cost = if last_dispatched == Some(customer.id) {
                0
            } else if last_dispatched.is_none() {
                0
            } else {
                switch_cost
            };

            let work_at = now.checked_add(cost).ok_or(Error::TimeOverflow {
                customer: customer.id,
                time: now,
            })?;
            let finish = work_at
                .checked_add(selected.remaining)
                .ok_or(Error::TimeOverflow {
                    customer: customer.id,
                    time: now,
                })?;

            let kind = if started[selected.index] {
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
            started[selected.index] = true;
            last_dispatched = Some(customer.id);
            events.push(Event {
                time: now,
                customer: customer.id,
                kind,
            });

            if finish == now {
                events.push(Event {
                    time: now,
                    customer: customer.id,
                    kind: Kind::Finish,
                });
                finished_count += 1;
            } else {
                running = Some(Running {
                    index: selected.index,
                    remaining: selected.remaining,
                    work_at,
                    finish,
                });
            }
        }
    }

    Ok(events)
}

fn main() {}