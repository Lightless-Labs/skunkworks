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
    arrivals.sort_by(|a, b| a.arrival.cmp(&b.arrival).then_with(|| b.id.cmp(&a.id)));

    let mut events = Vec::with_capacity(customers.len().saturating_mul(3));
    let mut queue: VecDeque<&Customer> = VecDeque::new();
    let mut next_arrival = 0usize;
    let mut active: Option<(u32, u64)> = None;
    let mut previous_finish: Option<u64> = None;
    let mut finished = 0usize;

    while finished < customers.len() {
        let next_time = match (active, arrivals.get(next_arrival)) {
            (Some((_, finish)), Some(customer)) => finish.min(customer.arrival),
            (Some((_, finish)), None) => finish,
            (None, Some(customer)) => customer.arrival,
            (None, None) => break,
        };

        if let Some((id, finish)) = active {
            if finish == next_time {
                events.push(Event {
                    time: next_time,
                    customer: id,
                    kind: Kind::Finish,
                });
                active = None;
                finished += 1;
                previous_finish = Some(next_time);
            }
        }

        while next_arrival < arrivals.len() && arrivals[next_arrival].arrival == next_time {
            let customer = arrivals[next_arrival];
            events.push(Event {
                time: next_time,
                customer: customer.id,
                kind: Kind::Arrive,
            });
            queue.push_back(customer);
            next_arrival += 1;
        }

        while active.is_none() {
            let Some(customer) = queue.pop_front() else {
                break;
            };

            let wake = previous_finish.map_or(true, |finish| next_time > finish);
            let base_finish = next_time.checked_add(customer.service).ok_or(
                Error::TimeOverflow {
                    customer: customer.id,
                    time: next_time,
                },
            )?;
            let finish = base_finish
                .checked_add(if wake { wake_cost } else { 0 })
                .ok_or(Error::TimeOverflow {
                    customer: customer.id,
                    time: next_time,
                })?;

            events.push(Event {
                time: next_time,
                customer: customer.id,
                kind: Kind::Start { finish, wake },
            });

            if finish == next_time {
                events.push(Event {
                    time: next_time,
                    customer: customer.id,
                    kind: Kind::Finish,
                });
                finished += 1;
                previous_finish = Some(next_time);
            } else {
                active = Some((customer.id, finish));
            }
        }
    }

    Ok(events)
}