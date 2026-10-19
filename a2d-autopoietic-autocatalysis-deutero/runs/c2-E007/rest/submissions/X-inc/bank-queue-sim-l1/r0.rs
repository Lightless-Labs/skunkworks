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
    let mut completion: Option<(&Customer, u64)> = None;
    let mut previous_finish: Option<u64> = None;

    loop {
        let next_arrival_time = arrivals.get(next_arrival).map(|c| c.arrival);
        let next_completion_time = completion.map(|(_, time)| time);

        let tick = match (next_arrival_time, next_completion_time) {
            (None, None) => break,
            (Some(a), None) => a,
            (None, Some(f)) => f,
            (Some(a), Some(f)) => a.min(f),
        };
        now = tick;

        if next_completion_time == Some(tick) {
            let (customer, _) = completion.take().expect("completion exists");
            events.push(Event {
                time: tick,
                customer: customer.id,
                kind: Kind::Finish,
            });
            previous_finish = Some(tick);
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

        if completion.is_none() {
            if let Some(customer) = waiting.pop_front() {
                let wake = previous_finish.map_or(true, |finish| tick > finish);
                let service_finish = tick.checked_add(customer.service).ok_or(
                    Error::TimeOverflow {
                        customer: customer.id,
                        time: tick,
                    },
                )?;
                let finish = if wake {
                    service_finish.checked_add(wake_cost).ok_or(Error::TimeOverflow {
                        customer: customer.id,
                        time: tick,
                    })?
                } else {
                    service_finish
                };

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
                } else {
                    completion = Some((customer, finish));
                }

                // Zero-duration occupations can chain at this tick, after the
                // complete arrival cohort has already been recorded.
                while completion.is_none() {
                    let Some(customer) = waiting.pop_front() else {
                        break;
                    };
                    let wake = previous_finish.map_or(true, |finish| tick > finish);
                    let service_finish = tick.checked_add(customer.service).ok_or(
                        Error::TimeOverflow {
                            customer: customer.id,
                            time: tick,
                        },
                    )?;
                    let finish = if wake {
                        service_finish.checked_add(wake_cost).ok_or(
                            Error::TimeOverflow {
                                customer: customer.id,
                                time: tick,
                            },
                        )?
                    } else {
                        service_finish
                    };

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
                    } else {
                        completion = Some((customer, finish));
                    }
                }
            }
        }
    }

    Ok(events)
}