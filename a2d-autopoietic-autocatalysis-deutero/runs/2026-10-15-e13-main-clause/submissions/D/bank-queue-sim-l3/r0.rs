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
    pause_time: u64,
}

struct Running {
    customer: Customer,
    work_at: u64,
    finish: u64,
}

fn waiting_precedes(a: &Waiting, b: &Waiting) -> bool {
    if a.customer.priority != b.customer.priority {
        return a.customer.priority < b.customer.priority;
    }

    match (a.started, b.started) {
        (true, false) => true,
        (false, true) => false,
        (true, true) => {
            (a.pause_time, a.customer.id) < (b.pause_time, b.customer.id)
        }
        (false, false) => {
            a.customer.arrival < b.customer.arrival
                || (a.customer.arrival == b.customer.arrival
                    && a.customer.id > b.customer.id)
        }
    }
}

fn pop_best(waiting: &mut Vec<Waiting>) -> Option<Waiting> {
    let mut best = 0;
    if waiting.is_empty() {
        return None;
    }

    for i in 1..waiting.len() {
        if waiting_precedes(&waiting[i], &waiting[best]) {
            best = i;
        }
    }
    Some(waiting.remove(best))
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
    let mut waiting = Vec::new();
    let mut running: Option<Running> = None;
    let mut next_arrival = 0usize;
    let mut last_dispatched = None;
    let mut finished_count = 0usize;
    let mut time = 0u64;

    while finished_count < customers.len() {
        // Process a scheduled completion before arrivals at this tick.
        if running.as_ref().is_some_and(|r| r.finish == time) {
            let done = running.take().unwrap();
            events.push(Event {
                time,
                customer: done.customer.id,
                kind: Kind::Finish,
            });
            finished_count += 1;
        }

        // Add the complete arrival cohort before making dispatch decisions.
        while next_arrival < arrivals.len() && arrivals[next_arrival].arrival == time {
            let customer = arrivals[next_arrival].clone();
            events.push(Event {
                time,
                customer: customer.id,
                kind: Kind::Arrive,
            });
            waiting.push(Waiting {
                remaining: customer.service,
                customer,
                started: false,
                pause_time: 0,
            });
            next_arrival += 1;
        }

        // At a work boundary, reconsider preemption after processing arrivals.
        if let Some(r) = running.as_ref() {
            if time >= r.work_at {
                if let Some(best) = waiting
                    .iter()
                    .min_by(|a, b| {
                        if waiting_precedes(a, b) {
                            std::cmp::Ordering::Less
                        } else if waiting_precedes(b, a) {
                            std::cmp::Ordering::Greater
                        } else {
                            std::cmp::Ordering::Equal
                        }
                    })
                {
                    if best.customer.priority < r.customer.priority {
                        let by = best.customer.id;
                        let paused = running.take().unwrap();
                        let remaining = paused.finish - time;
                        events.push(Event {
                            time,
                            customer: paused.customer.id,
                            kind: Kind::Preempt { remaining, by },
                        });
                        waiting.push(Waiting {
                            customer: paused.customer,
                            remaining,
                            started: true,
                            pause_time: time,
                        });
                    }
                }
            }
        }

        // Dispatch until the server is occupied or no waiter remains.
        if running.is_none() {
            while let Some(mut selected) = pop_best(&mut waiting) {
                let cost = if last_dispatched.is_none()
                    || last_dispatched == Some(selected.customer.id)
                {
                    0
                } else {
                    switch_cost
                };

                let work_at = time.checked_add(cost).ok_or(Error::TimeOverflow {
                    customer: selected.customer.id,
                    time,
                })?;
                let finish = work_at.checked_add(selected.remaining).ok_or(
                    Error::TimeOverflow {
                        customer: selected.customer.id,
                        time,
                    },
                )?;

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
                    customer: selected.customer.id,
                    kind,
                });
                last_dispatched = Some(selected.customer.id);
                selected.started = true;

                if finish == time {
                    events.push(Event {
                        time,
                        customer: selected.customer.id,
                        kind: Kind::Finish,
                    });
                    finished_count += 1;
                } else {
                    running = Some(Running {
                        customer: selected.customer,
                        work_at,
                        finish,
                    });
                    break;
                }
            }
        }

        if finished_count == customers.len() {
            break;
        }

        // Advance directly to the next actual event or work boundary.
        let mut next_time: Option<u64> =
            arrivals.get(next_arrival).map(|customer| customer.arrival);

        if let Some(r) = running.as_ref() {
            next_time = Some(next_time.map_or(r.finish, |t| t.min(r.finish)));
            if r.work_at > time {
                next_time = Some(next_time.map_or(r.work_at, |t| t.min(r.work_at)));
            }
        }

        match next_time {
            Some(t) => time = t,
            None => break,
        }
    }

    Ok(events)
}