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
    Start { remaining: u64, work_at: u64, finish: u64 },
    Resume { remaining: u64, work_at: u64, finish: u64 },
    Preempt { remaining: u64, by: u32 },
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

fn waiter_before(a: &Waiting, b: &Waiting, customers: &[Customer]) -> bool {
    let ca = &customers[a.index];
    let cb = &customers[b.index];

    if ca.priority != cb.priority {
        return ca.priority < cb.priority;
    }

    match (a.paused_at, b.paused_at) {
        (Some(ta), Some(tb)) => (ta, ca.id) < (tb, cb.id),
        (Some(_), None) => true,
        (None, Some(_)) => false,
        (None, None) => {
            ca.arrival < cb.arrival || (ca.arrival == cb.arrival && ca.id > cb.id)
        }
    }
}

fn pop_best(waiting: &mut Vec<Waiting>, customers: &[Customer]) -> Option<Waiting> {
    let best = waiting
        .iter()
        .enumerate()
        .min_by(|(_, a), (_, b)| {
            if waiter_before(a, b, customers) {
                std::cmp::Ordering::Less
            } else if waiter_before(b, a, customers) {
                std::cmp::Ordering::Greater
            } else {
                std::cmp::Ordering::Equal
            }
        })
        .map(|(i, _)| i)?;
    Some(waiting.swap_remove(best))
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

    let mut arrivals: Vec<usize> = (0..customers.len()).collect();
    arrivals.sort_by(|&a, &b| {
        let ca = &customers[a];
        let cb = &customers[b];
        ca.arrival
            .cmp(&cb.arrival)
            .then_with(|| ca.priority.cmp(&cb.priority))
            .then_with(|| cb.id.cmp(&ca.id))
    });

    let mut trace = Vec::new();
    let mut waiting: Vec<Waiting> = Vec::new();
    let mut running: Option<Running> = None;
    let mut next_arrival = 0;
    let mut now = 0u64;
    let mut last_dispatched: Option<u32> = None;
    let mut started = vec![false; customers.len()];
    let mut finished_count = 0;

    while finished_count < customers.len() {
        let next_time = [
            arrivals.get(next_arrival).map(|&i| customers[i].arrival),
            running.as_ref().map(|r| r.finish),
            running.as_ref().map(|r| r.work_at).filter(|&t| t > now),
        ]
        .into_iter()
        .flatten()
        .filter(|&t| t >= now)
        .min();

        let Some(tick) = next_time else {
            break;
        };
        now = tick;

        if running.as_ref().is_some_and(|r| r.finish == now) {
            let r = running.take().unwrap();
            trace.push(Event {
                time: now,
                customer: customers[r.index].id,
                kind: Kind::Finish,
            });
            finished_count += 1;
        }

        while next_arrival < arrivals.len()
            && customers[arrivals[next_arrival]].arrival == now
        {
            let index = arrivals[next_arrival];
            let customer = &customers[index];
            trace.push(Event {
                time: now,
                customer: customer.id,
                kind: Kind::Arrive,
            });
            waiting.push(Waiting {
                index,
                paused_at: None,
                remaining: customer.service,
            });
            next_arrival += 1;
        }

        if let Some(r) = running.as_ref() {
            if now >= r.work_at {
                let best_priority = waiting
                    .iter()
                    .map(|w| customers[w.index].priority)
                    .min();
                if best_priority.is_some_and(|p| p < customers[r.index].priority) {
                    let best = pop_best(&mut waiting, customers).unwrap();
                    let by = customers[best.index].id;
                    let r = running.take().unwrap();
                    let remaining = r.finish - now;
                    trace.push(Event {
                        time: now,
                        customer: customers[r.index].id,
                        kind: Kind::Preempt { remaining, by },
                    });
                    waiting.push(Waiting {
                        index: r.index,
                        paused_at: Some(now),
                        remaining,
                    });
                    waiting.push(best);
                }
            }
        }

        loop {
            if running.is_some() {
                break;
            }
            let Some(waiter) = pop_best(&mut waiting, customers) else {
                break;
            };
            let customer = &customers[waiter.index];
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
            let finish = work_at.checked_add(waiter.remaining).ok_or(Error::TimeOverflow {
                customer: customer.id,
                time: now,
            })?;

            let kind = if started[waiter.index] {
                Kind::Resume {
                    remaining: waiter.remaining,
                    work_at,
                    finish,
                }
            } else {
                Kind::Start {
                    remaining: waiter.remaining,
                    work_at,
                    finish,
                }
            };
            started[waiter.index] = true;
            last_dispatched = Some(customer.id);
            trace.push(Event {
                time: now,
                customer: customer.id,
                kind,
            });

            if finish == now {
                trace.push(Event {
                    time: now,
                    customer: customer.id,
                    kind: Kind::Finish,
                });
                finished_count += 1;
                continue;
            }

            running = Some(Running {
                index: waiter.index,
                remaining: waiter.remaining,
                work_at,
                finish,
            });
        }
    }

    Ok(trace)
}