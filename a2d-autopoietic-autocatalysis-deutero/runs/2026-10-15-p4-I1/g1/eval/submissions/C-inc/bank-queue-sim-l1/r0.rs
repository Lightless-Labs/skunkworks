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

    let mut events = Vec::with_capacity(customers.len() * 3);
    let mut waiting = VecDeque::new();
    let mut next_arrival = 0;
    let mut running: Option<(usize, u64)> = None;
    let mut time = 0u64;
    let mut previous_finish: Option<u64> = None;
    let mut completed = 0usize;

    while completed < customers.len() {
        let next_arrival_time = arrivals
            .get(next_arrival)
            .map(|&i| customers[i].arrival);
        let next_finish_time = running.map(|(_, finish)| finish);

        let tick = match (next_arrival_time, next_finish_time) {
            (Some(a), Some(f)) => a.min(f),
            (Some(a), None) => a,
            (None, Some(f)) => f,
            (None, None) => break,
        };
        time = tick;

        if let Some((index, finish)) = running {
            if finish == time {
                events.push(Event {
                    time,
                    customer: customers[index].id,
                    kind: Kind::Finish,
                });
                running = None;
                completed += 1;
                previous_finish = Some(time);
            }
        }

        while next_arrival < arrivals.len()
            && customers[arrivals[next_arrival]].arrival == time
        {
            let index = arrivals[next_arrival];
            events.push(Event {
                time,
                customer: customers[index].id,
                kind: Kind::Arrive,
            });
            waiting.push_back(index);
            next_arrival += 1;
        }

        if running.is_none() {
            if let Some(index) = waiting.pop_front() {
                let customer = &customers[index];
                let wake = previous_finish.map_or(true, |finish| time > finish);
                let finish = time
                    .checked_add(customer.service)
                    .and_then(|t| t.checked_add(if wake { wake_cost } else { 0 }))
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
                    completed += 1;
                    previous_finish = Some(time);
                } else {
                    running = Some((index, finish));
                }
            }
        }
    }

    Ok(events)
}

fn main() {}