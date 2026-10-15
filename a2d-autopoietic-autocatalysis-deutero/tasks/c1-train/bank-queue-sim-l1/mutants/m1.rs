use std::collections::{HashSet, VecDeque};
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
    let mut seen = HashSet::new();
    for (index, c) in customers.iter().enumerate() {
        if c.id == 0 {
            return Err(Error::ZeroId { index });
        }
        if !seen.insert(c.id) {
            return Err(Error::DuplicateId { index, id: c.id });
        }
    }
    let mut order: Vec<usize> = (0..customers.len()).collect();
    order.sort_by(|&a, &b| {
        customers[a]
            .arrival
            .cmp(&customers[b].arrival)
            .then(customers[b].id.cmp(&customers[a].id))
    });
    let mut next = 0;
    let mut queue = VecDeque::new();
    let mut active: Option<(usize, u64)> = None;
    let mut last_finish: Option<u64> = None;
    let mut events = Vec::new();
    while next < order.len() || active.is_some() || !queue.is_empty() {
        let arrival = order.get(next).map(|&i| customers[i].arrival);
        let finish = active.map(|(_, t)| t);
        let time = match (arrival, finish) {
            (Some(a), Some(b)) => a.min(b),
            (Some(a), None) => a,
            (None, Some(b)) => b,
            (None, None) => last_finish.unwrap(),
        };
        if let Some((i, t)) = active {
            if t == time {
                events.push(Event {
                    time,
                    customer: customers[i].id,
                    kind: Kind::Finish,
                });
                active = None;
                last_finish = Some(time);
            }
        }
        while next < order.len() && customers[order[next]].arrival == time {
            let i = order[next];
            next += 1;
            events.push(Event {
                time,
                customer: customers[i].id,
                kind: Kind::Arrive,
            });
            queue.push_back(i);
        }
        while active.is_none() {
            let Some(i) = queue.pop_front() else {
                break;
            };
            let c = &customers[i];
            let wake = last_finish.map_or(true, |t| time >= t);
            let finish = time
                .checked_add(c.service)
                .and_then(|t| t.checked_add(if wake { wake_cost } else { 0 }))
                .ok_or(Error::TimeOverflow {
                    customer: c.id,
                    time,
                })?;
            events.push(Event {
                time,
                customer: c.id,
                kind: Kind::Start { finish, wake },
            });
            if finish == time {
                events.push(Event {
                    time,
                    customer: c.id,
                    kind: Kind::Finish,
                });
                last_finish = Some(time);
            } else {
                active = Some((i, finish));
            }
        }
    }
    Ok(events)
}
