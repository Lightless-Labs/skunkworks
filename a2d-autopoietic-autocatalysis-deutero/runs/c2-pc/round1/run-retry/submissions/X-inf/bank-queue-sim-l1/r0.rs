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
    let mut next_arrival = 0;
    let mut current_time = 0u64;
    let mut active: Option<(&Customer, u64)> = None;
    let mut previous_finish: Option<u64> = None;
    let mut first_dispatch = true;

    while next_arrival < arrivals.len() || !waiting.is_empty() || active.is_some() {
        let next_arrival_time = arrivals
            .get(next_arrival)
            .map(|customer| customer.arrival);
        let next_finish_time = active.map(|(_, finish)| finish);

        let tick = match (next_arrival_time, next_finish_time) {
            (Some(arrival), Some(finish)) => arrival.min(finish),
            (Some(arrival), None) => arrival,
            (None, Some(finish)) => finish,
            (None, None) => break,
        };
        current_time = tick;

        if let Some((customer, finish)) = active {
            if finish == tick {
                events.push(Event {
                    time: tick,
                    customer: customer.id,
                    kind: Kind::Finish,
                });
                active = None;
                previous_finish = Some(tick);
            }
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

        if active.is_none() {
            if let Some(customer) = waiting.pop_front() {
                let wake = first_dispatch
                    || previous_finish.map_or(false, |finish| tick > finish);
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
                first_dispatch = false;

                if finish == tick {
                    events.push(Event {
                        time: tick,
                        customer: customer.id,
                        kind: Kind::Finish,
                    });
                    previous_finish = Some(tick);
                } else {
                    active = Some((customer, finish));
                }
            }
        }

        // A zero-duration occupation finishes inline; keep dispatching at this
        // tick before advancing to any later event.
        while active.is_none() && !waiting.is_empty() {
            let customer = waiting.pop_front().expect("queue is nonempty");
            let wake = first_dispatch
                || previous_finish.map_or(false, |finish| tick > finish);
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
            first_dispatch = false;

            if finish == tick {
                events.push(Event {
                    time: tick,
                    customer: customer.id,
                    kind: Kind::Finish,
                });
                previous_finish = Some(tick);
            } else {
                active = Some((customer, finish));
            }
        }
    }

    Ok(events)
}