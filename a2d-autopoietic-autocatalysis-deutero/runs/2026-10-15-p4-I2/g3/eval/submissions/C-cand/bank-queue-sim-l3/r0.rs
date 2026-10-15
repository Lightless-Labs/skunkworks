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
    started: bool,
    paused_at: Option<u64>,
    remaining: u64,
}

#[derive(Clone)]
struct Running {
    customer: Customer,
    remaining: u64,
    work_at: u64,
    finish: u64,
}

fn waiting_precedes(a: &Waiting, b: &Waiting) -> bool {
    if a.customer.priority != b.customer.priority {
        return a.customer.priority < b.customer.priority;
    }

    match (a.paused_at, b.paused_at) {
        (Some(at_a), Some(at_b)) if at_a != at_b => at_a < at_b,
        (Some(_), None) => true,
        (None, Some(_)) => false,
        _ => {
            if a.customer.arrival != b.customer.arrival {
                a.customer.arrival < b.customer.arrival
            } else {
                a.customer.id > b.customer.id
            }
        }
    }
}

fn best_waiter_index(waiting: &[Waiting]) -> Option<usize> {
    waiting
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
        .map(|(index, _)| index)
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

    let mut trace = Vec::new();
    let mut waiting: Vec<Waiting> = Vec::new();
    let mut running: Option<Running> = None;
    let mut next_arrival = 0usize;
    let mut last_dispatched: Option<u32> = None;

    while next_arrival < arrivals.len() || running.is_some() || !waiting.is_empty() {
        let mut tick = u64::MAX;
        let mut has_tick = false;

        if next_arrival < arrivals.len() {
            tick = arrivals[next_arrival].arrival;
            has_tick = true;
        }

        if let Some(active) = &running {
            if !has_tick || active.finish < tick {
                tick = active.finish;
                has_tick = true;
            }
            if active.work_at > tick || !has_tick {
                tick = active.work_at;
                has_tick = true;
            }
        }

        if !has_tick {
            break;
        }

        if running.as_ref().is_some_and(|active| active.finish == tick) {
            let active = running.take().unwrap();
            trace.push(Event {
                time: tick,
                customer: active.customer.id,
                kind: Kind::Finish,
            });
        }

        while next_arrival < arrivals.len() && arrivals[next_arrival].arrival == tick {
            let customer = arrivals[next_arrival].clone();
            trace.push(Event {
                time: tick,
                customer: customer.id,
                kind: Kind::Arrive,
            });
            waiting.push(Waiting {
                remaining: customer.service,
                customer,
                started: false,
                paused_at: None,
            });
            next_arrival += 1;
        }

        if let Some(active) = &running {
            if tick >= active.work_at {
                if let Some(index) = best_waiter_index(&waiting) {
                    if waiting[index].customer.priority < active.customer.priority {
                        let by = waiting[index].customer.id;
                        let active = running.take().unwrap();
                        let remaining = active.finish - tick;
                        trace.push(Event {
                            time: tick,
                            customer: active.customer.id,
                            kind: Kind::Preempt { remaining, by },
                        });
                        waiting.push(Waiting {
                            customer: active.customer,
                            started: true,
                            paused_at: Some(tick),
                            remaining,
                        });
                    }
                }
            }
        }

        while running.is_none() {
            let Some(index) = best_waiter_index(&waiting) else {
                break;
            };
            let selected = waiting.remove(index);
            let customer = selected.customer;
            let cost = if last_dispatched.is_some_and(|id| id != customer.id) {
                switch_cost
            } else {
                0
            };
            let work_at = tick.checked_add(cost).ok_or(Error::TimeOverflow {
                customer: customer.id,
                time: tick,
            })?;
            let finish = work_at.checked_add(selected.remaining).ok_or(Error::TimeOverflow {
                customer: customer.id,
                time: tick,
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
                time: tick,
                customer: customer.id,
                kind,
            });
            last_dispatched = Some(customer.id);

            if finish == tick {
                trace.push(Event {
                    time: tick,
                    customer: customer.id,
                    kind: Kind::Finish,
                });
            } else {
                running = Some(Running {
                    customer,
                    remaining: selected.remaining,
                    work_at,
                    finish,
                });
            }
        }
    }

    Ok(trace)
}

fn main() {}