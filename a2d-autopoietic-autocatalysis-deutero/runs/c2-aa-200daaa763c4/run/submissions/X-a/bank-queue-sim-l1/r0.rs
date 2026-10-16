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
    let mut waiting = std::collections::VecDeque::new();
    let mut next_arrival = 0usize;
    let mut now = 0u64;
    let mut finish: Option<(u64, u32)> = None;
    let mut previous_finish: Option<u64> = None;
    let mut first_dispatch = true;
    let mut finished_count = 0usize;

    while finished_count < customers.len() {
        let next_arrival_time = arrivals.get(next_arrival).map(|c| c.arrival);
        let next_finish_time = finish.map(|(time, _)| time);

        let tick = match (next_arrival_time, next_finish_time) {
            (Some(a), Some(f)) => a.min(f),
            (Some(a), None) => a,
            (None, Some(f)) => f,
            (None, None) => break,
        };
        now = tick;

        if let Some((time, id)) = finish {
            if time == tick {
                events.push(Event {
                    time: tick,
                    customer: id,
                    kind: Kind::Finish,
                });
                finish = None;
                previous_finish = Some(tick);
                finished_count += 1;
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

        if finish.is_none() {
            if let Some(customer) = waiting.pop_front() {
                let wake = first_dispatch || previous_finish.is_some_and(|t| tick > t);
                let service_finish = tick.checked_add(customer.service).ok_or(
                    Error::TimeOverflow {
                        customer: customer.id,
                        time: tick,
                    },
                )?;
                let finish_time = service_finish
                    .checked_add(if wake { wake_cost } else { 0 })
                    .ok_or(Error::TimeOverflow {
                        customer: customer.id,
                        time: tick,
                    })?;

                events.push(Event {
                    time: tick,
                    customer: customer.id,
                    kind: Kind::Start {
                        finish: finish_time,
                        wake,
                    },
                });
                first_dispatch = false;

                if finish_time == tick {
                    events.push(Event {
                        time: tick,
                        customer: customer.id,
                        kind: Kind::Finish,
                    });
                    previous_finish = Some(tick);
                    finished_count += 1;
                } else {
                    finish = Some((finish_time, customer.id));
                }
            }
        }

        // A zero-duration customer can finish inline; dispatch further queued
        // customers at the same tick before advancing time.
        while finish.is_none() && !waiting.is_empty() {
            let customer = waiting.pop_front().expect("queue is nonempty");
            let wake = first_dispatch || previous_finish.is_some_and(|t| tick > t);
            let service_finish = tick.checked_add(customer.service).ok_or(
                Error::TimeOverflow {
                    customer: customer.id,
                    time: tick,
                },
            )?;
            let finish_time = service_finish
                .checked_add(if wake { wake_cost } else { 0 })
                .ok_or(Error::TimeOverflow {
                    customer: customer.id,
                    time: tick,
                })?;

            events.push(Event {
                time: tick,
                customer: customer.id,
                kind: Kind::Start {
                    finish: finish_time,
                    wake,
                },
            });
            first_dispatch = false;

            if finish_time == tick {
                events.push(Event {
                    time: tick,
                    customer: customer.id,
                    kind: Kind::Finish,
                });
                previous_finish = Some(tick);
                finished_count += 1;
            } else {
                finish = Some((finish_time, customer.id));
            }
        }
    }

    Ok(events)
}