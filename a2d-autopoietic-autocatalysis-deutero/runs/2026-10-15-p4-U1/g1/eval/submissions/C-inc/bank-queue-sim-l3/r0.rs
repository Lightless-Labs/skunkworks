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
    use std::cmp::Ordering;

    let ca = &customers[a.index];
    let cb = &customers[b.index];

    ca.priority
        .cmp(&cb.priority)
        .then_with(|| match (a.paused_at, b.paused_at) {
            (Some(ta), Some(tb)) => ta.cmp(&tb).then_with(|| ca.id.cmp(&cb.id)),
            (Some(_), None) => Ordering::Less,
            (None, Some(_)) => Ordering::Greater,
            (None, None) => ca
                .arrival
                .cmp(&cb.arrival)
                .then_with(|| cb.id.cmp(&ca.id)),
        })
}

fn take_best(waiting: &mut Vec<Waiting>, customers: &[Customer]) -> Option<Waiting> {
    let best = waiting
        .iter()
        .enumerate()
        .min_by(|(_, a), (_, b)| waiting_cmp(a, b, customers))
        .map(|(i, _)| i)?;
    Some(waiting.swap_remove(best))
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
    let mut next_arrival = 0usize;
    let mut current_time = customers[arrivals[0]].arrival;
    let mut last_dispatched: Option<u32> = None;
    let mut started = vec![false; customers.len()];
    let mut finished_count = 0usize;

    while finished_count < customers.len() {
        if let Some(active) = running.as_ref() {
            if active.finish == current_time {
                let active = running.take().unwrap();
                let customer = &customers[active.index];
                events.push(Event {
                    time: current_time,
                    customer: customer.id,
                    kind: Kind::Finish,
                });
                finished_count += 1;
            }
        }

        while next_arrival < arrivals.len()
            && customers[arrivals[next_arrival]].arrival == current_time
        {
            let index = arrivals[next_arrival];
            let customer = &customers[index];
            events.push(Event {
                time: current_time,
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

        if let Some(active) = running.as_ref() {
            if current_time >= active.work_at {
                if let Some(best) = waiting.iter().min_by(|a, b| waiting_cmp(a, b, customers)) {
                    let active_priority = customers[active.index].priority;
                    let best_customer = &customers[best.index];
                    if best_customer.priority < active_priority {
                        let best_id = best_customer.id;
                        let active = running.take().unwrap();
                        let customer = &customers[active.index];
                        let remaining = active.finish - current_time;
                        events.push(Event {
                            time: current_time,
                            customer: customer.id,
                            kind: Kind::Preempt {
                                remaining,
                                by: best_id,
                            },
                        });
                        waiting.push(Waiting {
                            index: active.index,
                            paused_at: Some(current_time),
                            remaining,
                        });
                    }
                }
            }
        }

        if running.is_none() {
            while let Some(chosen) = take_best(&mut waiting, customers) {
                let customer = &customers[chosen.index];
                let switch = if last_dispatched == Some(customer.id) {
                    0
                } else if last_dispatched.is_none() {
                    0
                } else {
                    switch_cost
                };

                let work_at = current_time.checked_add(switch).ok_or(Error::TimeOverflow {
                    customer: customer.id,
                    time: current_time,
                })?;
                let finish = work_at.checked_add(chosen.remaining).ok_or(Error::TimeOverflow {
                    customer: customer.id,
                    time: current_time,
                })?;

                let kind = if started[chosen.index] {
                    Kind::Resume {
                        remaining: chosen.remaining,
                        work_at,
                        finish,
                    }
                } else {
                    started[chosen.index] = true;
                    Kind::Start {
                        remaining: chosen.remaining,
                        work_at,
                        finish,
                    }
                };

                events.push(Event {
                    time: current_time,
                    customer: customer.id,
                    kind,
                });
                last_dispatched = Some(customer.id);

                if finish == current_time {
                    events.push(Event {
                        time: current_time,
                        customer: customer.id,
                        kind: Kind::Finish,
                    });
                    finished_count += 1;
                    continue;
                }

                running = Some(Running {
                    index: chosen.index,
                    remaining: chosen.remaining,
                    work_at,
                    finish,
                });
                break;
            }
        }

        if finished_count == customers.len() {
            break;
        }

        let mut next_time: Option<u64> = None;
        if next_arrival < arrivals.len() {
            next_time = Some(customers[arrivals[next_arrival]].arrival);
        }
        if let Some(active) = running.as_ref() {
            for tick in [active.work_at, active.finish] {
                if tick > current_time {
                    next_time = Some(next_time.map_or(tick, |old| old.min(tick)));
                }
            }
        }

        match next_time {
            Some(time) => current_time = time,
            None => break,
        }
    }

    Ok(events)
}

fn main() {}