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
    let mut next_arrival = 0usize;
    let mut now = 0u64;
    let mut completion: Option<(u64, u32)> = None;
    let mut previous_finish: Option<u64> = None;

    while next_arrival < arrivals.len() || completion.is_some() || !waiting.is_empty() {
        let next_arrival_time = arrivals.get(next_arrival).map(|c| c.arrival);
        let next_completion_time = completion.map(|(time, _)| time);

        now = match (next_arrival_time, next_completion_time) {
            (Some(a), Some(f)) => a.min(f),
            (Some(a), None) => a,
            (None, Some(f)) => f,
            (None, None) => break,
        };

        if let Some((finish_time, customer_id)) = completion {
            if finish_time == now {
                events.push(Event {
                    time: now,
                    customer: customer_id,
                    kind: Kind::Finish,
                });
                completion = None;
                previous_finish = Some(now);
            }
        }

        while next_arrival < arrivals.len() && arrivals[next_arrival].arrival == now {
            let customer = arrivals[next_arrival];
            events.push(Event {
                time: now,
                customer: customer.id,
                kind: Kind::Arrive,
            });
            waiting.push_back(customer);
            next_arrival += 1;
        }

        if completion.is_none() {
            if let Some(customer) = waiting.pop_front() {
                let wake = previous_finish.map_or(true, |finish| now > finish);
                let service_finish = now.checked_add(customer.service).ok_or(
                    Error::TimeOverflow {
                        customer: customer.id,
                        time: now,
                    },
                )?;
                let finish = service_finish
                    .checked_add(if wake { wake_cost } else { 0 })
                    .ok_or(Error::TimeOverflow {
                        customer: customer.id,
                        time: now,
                    })?;

                events.push(Event {
                    time: now,
                    customer: customer.id,
                    kind: Kind::Start { finish, wake },
                });

                if finish == now {
                    events.push(Event {
                        time: now,
                        customer: customer.id,
                        kind: Kind::Finish,
                    });
                    previous_finish = Some(now);
                } else {
                    completion = Some((finish, customer.id));
                }
            }
        }

        // A zero-duration occupation finishes inline. Continue dispatching at
        // this tick before advancing to another arrival or completion.
        while completion.is_none() && !waiting.is_empty() {
            let customer = waiting.pop_front().expect("queue was checked as nonempty");
            let wake = previous_finish.map_or(true, |finish| now > finish);
            let service_finish = now.checked_add(customer.service).ok_or(
                Error::TimeOverflow {
                    customer: customer.id,
                    time: now,
                },
            )?;
            let finish = service_finish
                .checked_add(if wake { wake_cost } else { 0 })
                .ok_or(Error::TimeOverflow {
                    customer: customer.id,
                    time: now,
                })?;

            events.push(Event {
                time: now,
                customer: customer.id,
                kind: Kind::Start { finish, wake },
            });

            if finish == now {
                events.push(Event {
                    time: now,
                    customer: customer.id,
                    kind: Kind::Finish,
                });
                previous_finish = Some(now);
            } else {
                completion = Some((finish, customer.id));
            }
        }
    }

    Ok(events)
}

fn main() {}