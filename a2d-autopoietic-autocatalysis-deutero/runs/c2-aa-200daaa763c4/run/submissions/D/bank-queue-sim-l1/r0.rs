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

    let mut arrivals: Vec<usize> = (0..customers.len()).collect();
    arrivals.sort_by(|&a, &b| {
        customers[a]
            .arrival
            .cmp(&customers[b].arrival)
            .then_with(|| customers[b].id.cmp(&customers[a].id))
    });

    let mut events = Vec::with_capacity(customers.len().saturating_mul(3));
    let mut waiting: VecDeque<usize> = VecDeque::new();
    let mut next_arrival = 0;
    let mut active: Option<(usize, u64)> = None;
    let mut previous_finish: Option<u64> = None;

    while next_arrival < arrivals.len() || active.is_some() || !waiting.is_empty() {
        let next_arrival_time = arrivals
            .get(next_arrival)
            .map(|&i| customers[i].arrival);
        let next_finish_time = active.map(|(_, finish)| finish);

        let time = match (next_arrival_time, next_finish_time) {
            (Some(a), Some(f)) => a.min(f),
            (Some(a), None) => a,
            (None, Some(f)) => f,
            (None, None) => break,
        };

        if let Some((customer_index, finish)) = active {
            if finish == time {
                events.push(Event {
                    time,
                    customer: customers[customer_index].id,
                    kind: Kind::Finish,
                });
                active = None;
                previous_finish = Some(time);
            }
        }

        while next_arrival < arrivals.len()
            && customers[arrivals[next_arrival]].arrival == time
        {
            let customer_index = arrivals[next_arrival];
            events.push(Event {
                time,
                customer: customers[customer_index].id,
                kind: Kind::Arrive,
            });
            waiting.push_back(customer_index);
            next_arrival += 1;
        }

        if active.is_none() {
            if let Some(customer_index) = waiting.pop_front() {
                let customer = &customers[customer_index];
                let wake = previous_finish.map_or(true, |finish| time > finish);
                let service_finish = time.checked_add(customer.service).ok_or(
                    Error::TimeOverflow {
                        customer: customer.id,
                        time,
                    },
                )?;
                let finish = service_finish
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
                    previous_finish = Some(time);
                } else {
                    active = Some((customer_index, finish));
                }
            }
        }
    }

    Ok(events)
}