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
    started: bool,
}

#[derive(Clone)]
struct Running {
    index: usize,
    finish: u64,
    work_at: u64,
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

    let mut arrival_order: Vec<usize> = (0..customers.len()).collect();
    arrival_order.sort_by(|&a, &b| {
        customers[a]
            .arrival
            .cmp(&customers[b].arrival)
            .then_with(|| customers[a].priority.cmp(&customers[b].priority))
            .then_with(|| customers[b].id.cmp(&customers[a].id))
    });

    let mut trace = Vec::new();
    let mut waiting: Vec<Waiting> = Vec::new();
    let mut running: Option<Running> = None;
    let mut next_arrival = 0usize;
    let mut last_dispatched: Option<u32> = None;
    let mut finished_count = 0usize;
    let mut now = 0u64;

    while finished_count < customers.len() {
        // Complete scheduled work before processing arrivals at this tick.
        if let Some(active) = running.as_ref() {
            if active.finish == now {
                let index = active.index;
                trace.push(Event {
                    time: now,
                    customer: customers[index].id,
                    kind: Kind::Finish,
                });
                running = None;
                finished_count += 1;
            }
        }

        // Process the complete arrival cohort in the prescribed order.
        while next_arrival < arrival_order.len()
            && customers[arrival_order[next_arrival]].arrival == now
        {
            let index = arrival_order[next_arrival];
            trace.push(Event {
                time: now,
                customer: customers[index].id,
                kind: Kind::Arrive,
            });
            waiting.push(Waiting {
                index,
                paused_at: None,
                remaining: customers[index].service,
                started: false,
            });
            next_arrival += 1;
        }

        if let Some(active) = running.as_ref() {
            if now >= active.work_at {
                let mut best: Option<usize> = None;
                for (i, candidate) in waiting.iter().enumerate() {
                    if best.is_none_or(|j| {
                        waiting_cmp(candidate, &waiting[j], customers).is_lt()
                    }) {
                        best = Some(i);
                    }
                }

                if let Some(best_index) = best {
                    let active_priority = customers[active.index].priority;
                    let best_priority = customers[waiting[best_index].index].priority;
                    if best_priority < active_priority {
                        let active = running.take().unwrap();
                        let remaining = active.finish - now;
                        let by = customers[waiting[best_index].index].id;
                        trace.push(Event {
                            time: now,
                            customer: customers[active.index].id,
                            kind: Kind::Preempt { remaining, by },
                        });
                        waiting.push(Waiting {
                            index: active.index,
                            paused_at: Some(now),
                            remaining,
                            started: true,
                        });
                    }
                }
            }
        }

        // Dispatch until the server is occupied or no customer is waiting.
        while running.is_none() && !waiting.is_empty() {
            let best_index = (0..waiting.len())
                .min_by(|&a, &b| waiting_cmp(&waiting[a], &waiting[b], customers))
                .unwrap();
            let selected = waiting.remove(best_index);
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
                customer: customer.id,
                kind,
            });
            last_dispatched = Some(customer.id);

            if finish == now {
                trace.push(Event {
                    time: now,
                    customer: customer.id,
                    kind: Kind::Finish,
                });
                finished_count += 1;
            } else {
                running = Some(Running {
                    index: selected.index,
                    finish,
                    work_at,
                });
            }
        }

        if finished_count == customers.len() {
            break;
        }

        let mut next_time: Option<u64> = None;
        if next_arrival < arrival_order.len() {
            next_time = Some(customers[arrival_order[next_arrival]].arrival);
        }
        if let Some(active) = running.as_ref() {
            for time in [active.finish, active.work_at] {
                if time > now && next_time.is_none_or(|n| time < n) {
                    next_time = Some(time);
                }
            }
        }

        match next_time {
            Some(time) => now = time,
            None => break,
        }
    }

    Ok(trace)
}