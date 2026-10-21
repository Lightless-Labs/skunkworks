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

    let mut ids = HashSet::with_capacity(customers.len());
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
    let mut next_arrival = 0;
    let mut now = 0u64;
    let mut finish_time: Option<u64> = None;
    let mut last_finish: Option<u64> = None;
    let mut completed = 0usize;

    while completed < customers.len() {
        let next_arrival_time = arrivals.get(next_arrival).map(|c| c.arrival);
        let tick = match (finish_time, next_arrival_time) {
            (Some(finish), Some(arrival)) => finish.min(arrival),
            (Some(finish), None) => finish,
            (None, Some(arrival)) => arrival,
            (None, None) => break,
        };
        now = tick;

        if finish_time == Some(tick) {
            let customer_id = match events.last() {
                Some(Event {
                    customer,
                    kind: Kind::Start { finish, .. },
                    ..
                }) if *finish == tick => *customer,
                _ => {
                    // The currently occupied customer's ID is tracked by the latest
                    // unmatched Start event.
                    events
                        .iter()
                        .rev()
                        .find_map(|event| match event.kind {
                            Kind::Start { finish, .. } if finish == tick => Some(event.customer),
                            _ => None,
                        })
                        .expect("scheduled finish must have a corresponding start")
                }
            };
            events.push(Event {
                time: tick,
                customer: customer_id,
                kind: Kind::Finish,
            });
            finish_time = None;
            last_finish = Some(tick);
            completed += 1;
        }

        while next_arrival < arrivals.len() && arrivals[next_arrival].arrival == tick {
            let customer = arrivals[next_arrival];
            events.push(Event {
                time: tick,
                customer: customer.id,
                kind: Kind::Arrive,
            });
            waiting.push_back(customer);
            next_arrival += 1;
        }

        if finish_time.is_none() {
            while let Some(customer) = waiting.pop_front() {
                let wake = last_finish.map_or(true, |previous| tick > previous);
                let service_finish = tick.checked_add(customer.service).ok_or(
                    Error::TimeOverflow {
                        customer: customer.id,
                        time: tick,
                    },
                )?;
                let finish = service_finish
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
                    last_finish = Some(tick);
                    completed += 1;
                } else {
                    finish_time = Some(finish);
                    break;
                }
            }
        }
    }

    let _ = now;
    Ok(events)
}