#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Customer {
    pub id: u32,
    pub arrival: u64,
    pub service: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Kind {
    Arrive,
    Start { finish: u64, wake: bool },
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
    TimeOverflow { customer: u32, time: u64 },
}

pub fn simulate(customers: &[Customer], wake_cost: u64) -> Result<Vec<Event>, Error> {
    use std::collections::{HashSet, VecDeque};

    let mut seen = HashSet::with_capacity(customers.len());
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
    }

    if customers.is_empty() {
        return Ok(Vec::new());
    }

    let mut arrivals: Vec<&Customer> = customers.iter().collect();
    arrivals.sort_by(|a, b| {
        a.arrival
            .cmp(&b.arrival)
            .then_with(|| b.id.cmp(&a.id))
    });

    let mut events = Vec::with_capacity(customers.len() * 3);
    let mut waiting: VecDeque<&Customer> = VecDeque::new();
    let mut arrival_index = 0usize;
    let mut now = 0u64;
    let mut finish_time: Option<u64> = None;
    let mut previous_finish: Option<u64> = None;
    let mut finished_count = 0usize;

    while finished_count < customers.len() {
        let next_arrival = arrivals.get(arrival_index).map(|c| c.arrival);
        let next_tick = match (finish_time, next_arrival) {
            (Some(finish), Some(arrival)) => Some(finish.min(arrival)),
            (Some(finish), None) => Some(finish),
            (None, Some(arrival)) => Some(arrival),
            (None, None) => None,
        };

        let Some(tick) = next_tick else {
            break;
        };
        now = tick;

        if finish_time == Some(tick) {
            let customer_id = match events.last() {
                Some(Event {
                    customer,
                    kind: Kind::Start { .. },
                    ..
                }) => *customer,
                _ => {
                    // The active customer's ID is tracked by its most recent start.
                    events
                        .iter()
                        .rev()
                        .find_map(|event| match event.kind {
                            Kind::Start { .. } => Some(event.customer),
                            _ => None,
                        })
                        .expect("scheduled finish must have an active customer")
                }
            };
            events.push(Event {
                time: tick,
                customer: customer_id,
                kind: Kind::Finish,
            });
            finish_time = None;
            previous_finish = Some(tick);
            finished_count += 1;
        }

        while arrival_index < arrivals.len() && arrivals[arrival_index].arrival == tick {
            let customer = arrivals[arrival_index];
            events.push(Event {
                time: tick,
                customer: customer.id,
                kind: Kind::Arrive,
            });
            waiting.push_back(customer);
            arrival_index += 1;
        }

        if finish_time.is_none() {
            if let Some(customer) = waiting.pop_front() {
                let wake = previous_finish.map_or(true, |finish| tick > finish);
                let base_finish = tick.checked_add(customer.service).ok_or(
                    Error::TimeOverflow {
                        customer: customer.id,
                        time: tick,
                    },
                )?;
                let finish = base_finish
                    .checked_add(if wake { wake_cost } else { 0 })
                    .ok_or(Error::TimeOverflow {
                        customer: customer.id,
                        time: tick,
                    })?;

                events.push(Event {
                    time: tick,
                    customer: customer.id,
                    kind: Kind::Start { finish, wake },
                });

                if finish == tick {
                    events.push(Event {
                        time: tick,
                        customer: customer.id,
                        kind: Kind::Finish,
                    });
                    previous_finish = Some(tick);
                    finished_count += 1;
                } else {
                    finish_time = Some(finish);
                }

                while finish_time.is_none() && !waiting.is_empty() {
                    let customer = waiting.pop_front().expect("queue is nonempty");
                    let wake = previous_finish.map_or(true, |finish| tick > finish);
                    let base_finish = tick.checked_add(customer.service).ok_or(
                        Error::TimeOverflow {
                            customer: customer.id,
                            time: tick,
                        },
                    )?;
                    let finish = base_finish
                        .checked_add(if wake { wake_cost } else { 0 })
                        .ok_or(Error::TimeOverflow {
                            customer: customer.id,
                            time: tick,
                        })?;

                    events.push(Event {
                        time: tick,
                        customer: customer.id,
                        kind: Kind::Start { finish, wake },
                    });

                    if finish == tick {
                        events.push(Event {
                            time: tick,
                            customer: customer.id,
                            kind: Kind::Finish,
                        });
                        previous_finish = Some(tick);
                        finished_count += 1;
                    } else {
                        finish_time = Some(finish);
                    }
                }
            }
        }
    }

    Ok(events)
}