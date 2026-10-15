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

    let mut events = Vec::new();
    let mut waiting: VecDeque<&Customer> = VecDeque::new();
    let mut next_arrival = 0;
    let mut current_time = 0u64;
    let mut running: Option<(&Customer, u64)> = None;
    let mut previous_finish: Option<u64> = None;

    loop {
        let next_time = match (running, arrivals.get(next_arrival)) {
            (Some((_, finish)), Some(customer)) => finish.min(customer.arrival),
            (Some((_, finish)), None) => finish,
            (None, Some(customer)) => current_time.max(customer.arrival),
            (None, None) => break,
        };
        current_time = next_time;

        if let Some((customer, finish)) = running {
            if finish == current_time {
                events.push(Event {
                    time: current_time,
                    customer: customer.id,
                    kind: Kind::Finish,
                });
                running = None;
                previous_finish = Some(current_time);
            }
        }

        while next_arrival < arrivals.len()
            && arrivals[next_arrival].arrival == current_time
        {
            let customer = arrivals[next_arrival];
            events.push(Event {
                time: current_time,
                customer: customer.id,
                kind: Kind::Arrive,
            });
            waiting.push_back(customer);
            next_arrival += 1;
        }

        if running.is_none() {
            if let Some(customer) = waiting.pop_front() {
                let wake = previous_finish.map_or(true, |finish| current_time > finish);
                let service_finish = current_time.checked_add(customer.service).ok_or(
                    Error::TimeOverflow {
                        customer: customer.id,
                        time: current_time,
                    },
                )?;
                let finish = service_finish
                    .checked_add(if wake { wake_cost } else { 0 })
                    .ok_or(Error::TimeOverflow {
                        customer: customer.id,
                        time: current_time,
                    })?;

                events.push(Event {
                    time: current_time,
                    customer: customer.id,
                    kind: Kind::Start { finish, wake },
                });

                if finish == current_time {
                    events.push(Event {
                        time: current_time,
                        customer: customer.id,
                        kind: Kind::Finish,
                    });
                    running = None;
                    previous_finish = Some(current_time);
                } else {
                    running = Some((customer, finish));
                }
            }
        }
    }

    Ok(events)
}

fn main() {}