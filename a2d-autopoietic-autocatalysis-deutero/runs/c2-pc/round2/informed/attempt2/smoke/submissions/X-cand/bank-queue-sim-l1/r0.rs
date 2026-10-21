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

    let mut arrivals: Vec<Customer> = customers.to_vec();
    arrivals.sort_by(|a, b| {
        a.arrival
            .cmp(&b.arrival)
            .then_with(|| b.id.cmp(&a.id))
    });

    let mut events = Vec::with_capacity(customers.len().saturating_mul(3));
    let mut queue: VecDeque<Customer> = VecDeque::new();
    let mut cursor = 0usize;
    let mut active: Option<(u32, u64)> = None;
    let mut previous_finish: Option<u64> = None;
    let mut finished_count = 0usize;

    while finished_count < customers.len() {
        let next_arrival = arrivals.get(cursor).map(|c| c.arrival);
        let next_completion = active.map(|(_, finish)| finish);
        let time = match (next_arrival, next_completion) {
            (Some(arrival), Some(finish)) => arrival.min(finish),
            (Some(arrival), None) => arrival,
            (None, Some(finish)) => finish,
            (None, None) => break,
        };

        if let Some((id, finish)) = active {
            if finish == time {
                events.push(Event {
                    time,
                    customer: id,
                    kind: Kind::Finish,
                });
                active = None;
                finished_count += 1;
                previous_finish = Some(time);
            }
        }

        while cursor < arrivals.len() && arrivals[cursor].arrival == time {
            let customer = arrivals[cursor].clone();
            events.push(Event {
                time,
                customer: customer.id,
                kind: Kind::Arrive,
            });
            queue.push_back(customer);
            cursor += 1;
        }

        while active.is_none() {
            let Some(customer) = queue.pop_front() else {
                break;
            };
            let wake = previous_finish.map_or(true, |finish| time > finish);
            let finish = time
                .checked_add(customer.service)
                .ok_or(Error::TimeOverflow {
                    customer: customer.id,
                    time,
                })?
                .checked_add(if wake { wake_cost } else { 0 })
                .ok_or(Error::TimeOverflow {
                    customer: customer.id,
                    time,
                })?;

            events.push(Event {
                time,
                customer: customer.id,
                kind: Kind::Start { finish, wake },
            });

            if finish == time {
                events.push(Event {
                    time,
                    customer: customer.id,
                    kind: Kind::Finish,
                });
                finished_count += 1;
                previous_finish = Some(time);
            } else {
                active = Some((customer.id, finish));
            }
        }
    }

    Ok(events)
}