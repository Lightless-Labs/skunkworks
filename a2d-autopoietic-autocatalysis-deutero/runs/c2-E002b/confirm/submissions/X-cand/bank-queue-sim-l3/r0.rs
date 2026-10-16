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
    pause_time: Option<u64>,
    started: bool,
}

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

fn take_best(waiting: &mut Vec<Waiting>) -> Option<Waiting> {
    let index = (0..waiting.len()).min_by(|&a, &b| {
        if waiting_precedes(&waiting[a], &waiting[b]) {
            std::cmp::Ordering::Less
        } else if waiting_precedes(&waiting[b], &waiting[a]) {
            std::cmp::Ordering::Greater
        } else {
            std::cmp::Ordering::Equal
        }
    })?;
    Some(waiting.remove(index))
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

    let mut arrivals: Vec<Customer> = customers.to_vec();
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
    let mut last_dispatched: Option<u32> = None;
    let mut finished_count = 0usize;
    let mut now: Option<u64> = None;

    while finished_count < customers.len() {
        let next_time = match (
            arrivals.get(next_arrival).map(|c| c.arrival),
            running.as_ref().map(|r| r.finish),
            running.as_ref().map(|r| r.work_at),
            now,
        ) {
            (Some(a), Some(f), Some(w), _) => a.min(f).min(if w > now.unwrap_or(0) { w } else { u64::MAX }),
            (Some(a), Some(f), Some(w), Some(current)) => a.min(f).min(if w > current { w } else { u64::MAX }),
            (Some(a), Some(f), _, _) => a.min(f),
            (Some(a), None, _, _) => a,
            (None, Some(f), _, _) => f,
            (None, None, _, Some(current)) => {
                if let Some(r) = running.as_ref() {
                    if r.work_at > current {
                        r.work_at
                    } else {
                        break;
                    }
                } else {
                    break;
                }
            }
            (None, None, _, None) => break,
        };
        now = Some(next_time);

        if running.as_ref().is_some_and(|r| r.finish == next_time) {
            let completed = running.take().unwrap();
            events.push(Event {
                time: next_time,
                customer: completed.customer.id,
                kind: Kind::Finish,
            });
            finished_count += 1;
        }

        while next_arrival < arrivals.len() && arrivals[next_arrival].arrival == next_time {
            let customer = arrivals[next_arrival].clone();
            events.push(Event {
                time: next_time,
                customer: customer.id,
                kind: Kind::Arrive,
            });
            waiting.push(Waiting {
                remaining: customer.service,
                customer,
                pause_time: None,
                started: false,
            });
            next_arrival += 1;
        }

        if let Some(active) = running.as_ref() {
            if next_time >= active.work_at {
                let best_priority = waiting
                    .iter()
                    .map(|w| w.customer.priority)
                    .min();
                if best_priority.is_some_and(|p| p < active.customer.priority) {
                    let by = waiting
                        .iter()
                        .filter(|w| Some(w.customer.priority) == best_priority)
                        .min_by(|a, b| {
                            if waiting_precedes(a, b) {
                                std::cmp::Ordering::Less
                            } else if waiting_precedes(b, a) {
                                std::cmp::Ordering::Greater
                            } else {
                                std::cmp::Ordering::Equal
                            }
                        })
                        .unwrap()
                        .customer
                        .id;
                    let paused = running.take().unwrap();
                    let remaining = paused.finish - next_time;
                    events.push(Event {
                        time: next_time,
                        customer: paused.customer.id,
                        kind: Kind::Preempt { remaining, by },
                    });
                    waiting.push(Waiting {
                        customer: paused.customer,
                        remaining,
                        pause_time: Some(next_time),
                        started: true,
                    });
                }
            }
        }

        if running.is_none() {
            while let Some(selected) = take_best(&mut waiting) {
                let switch = if last_dispatched == Some(selected.customer.id) || last_dispatched.is_none() {
                    0
                } else {
                    switch_cost
                };
                let work_at = next_time.checked_add(switch).ok_or(Error::TimeOverflow {
                    customer: selected.customer.id,
                    time: next_time,
                })?;
                let finish = work_at.checked_add(selected.remaining).ok_or(Error::TimeOverflow {
                    customer: selected.customer.id,
                    time: next_time,
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
                events.push(Event {
                    time: next_time,
                    customer: selected.customer.id,
                    kind,
                });
                last_dispatched = Some(selected.customer.id);
                running = Some(Running {
                    customer: selected.customer,
                    remaining: selected.remaining,
                    work_at,
                    finish,
                });

                if finish == next_time {
                    let completed = running.take().unwrap();
                    events.push(Event {
                        time: next_time,
                        customer: completed.customer.id,
                        kind: Kind::Finish,
                    });
                    finished_count += 1;
                    continue;
                }
                break;
            }
        }
    }

    Ok(events)
}