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

    let mut arrivals: Vec<usize> = (0..customers.len()).collect();
    arrivals.sort_by(|&a, &b| {
        customers[a]
            .arrival
            .cmp(&customers[b].arrival)
            .then_with(|| customers[b].id.cmp(&customers[a].id))
    });

    let mut events = Vec::with_capacity(customers.len().saturating_mul(3));
    let mut waiting: VecDeque<usize> = VecDeque::new();
    let mut next_arrival = 0usize;
    let mut occupied: Option<(usize, u64)> = None;
    let mut previous_finish: Option<u64> = None;
    let mut first_dispatch = true;
    let mut completed = 0usize;

    while completed < customers.len() {
        let next_arrival_time = arrivals
            .get(next_arrival)
            .map(|&index| customers[index].arrival);
        let next_finish_time = occupied.map(|(_, finish)| finish);

        let tick = match (next_arrival_time, next_finish_time) {
            (Some(arrival), Some(finish)) => arrival.min(finish),
            (Some(arrival), None) => arrival,
            (None, Some(finish)) => finish,
            (None, None) => break,
        };

        if let Some((customer_index, finish)) = occupied {
            if finish == tick {
                events.push(Event {
                    time: tick,
                    customer: customers[customer_index].id,
                    kind: Kind::Finish,
                });
                occupied = None;
                completed += 1;
                previous_finish = Some(tick);
            }
        }

        while next_arrival < arrivals.len()
            && customers[arrivals[next_arrival]].arrival == tick
        {
            let customer_index = arrivals[next_arrival];
            events.push(Event {
                time: tick,
                customer: customers[customer_index].id,
                kind: Kind::Arrive,
            });
            waiting.push_back(customer_index);
            next_arrival += 1;
        }

        if occupied.is_none() {
            if let Some(customer_index) = waiting.pop_front() {
                let customer = &customers[customer_index];
                let wake =
                    first_dispatch || previous_finish.is_some_and(|finish| tick > finish);

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
                    completed += 1;
                    previous_finish = Some(tick);
                } else {
                    occupied = Some((customer_index, finish));
                }
            }
        }
    }

    Ok(events)
}