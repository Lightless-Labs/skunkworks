use std::collections::{HashSet, VecDeque};
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Customer {
    pub id: u32,
    pub arrival: u64,
    pub service: u64,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Server {
    pub id: u32,
    pub opens: u64,
    pub overhead: u64,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Event {
    Open {
        time: u64,
        server: u32,
    },
    Arrive {
        time: u64,
        customer: u32,
    },
    Start {
        time: u64,
        customer: u32,
        server: u32,
        finish: u64,
    },
    Finish {
        time: u64,
        customer: u32,
        server: u32,
    },
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Error {
    NoServers,
    TooManyServers {
        count: usize,
    },
    ZeroServerId {
        index: usize,
    },
    DuplicateServerId {
        index: usize,
        id: u32,
    },
    ZeroCustomerId {
        index: usize,
    },
    DuplicateCustomerId {
        index: usize,
        id: u32,
    },
    TimeOverflow {
        customer: u32,
        server: u32,
        time: u64,
    },
}
pub fn simulate(customers: &[Customer], servers: &[Server]) -> Result<Vec<Event>, Error> {
    if servers.is_empty() {
        return Err(Error::NoServers);
    }
    if servers.len() > 32 {
        return Err(Error::TooManyServers {
            count: servers.len(),
        });
    }
    let mut seen = HashSet::new();
    for (index, s) in servers.iter().enumerate() {
        if s.id == 0 {
            return Err(Error::ZeroServerId { index });
        }
        if !seen.insert(s.id) {
            return Err(Error::DuplicateServerId { index, id: s.id });
        }
    }
    seen.clear();
    for (index, c) in customers.iter().enumerate() {
        if c.id == 0 {
            return Err(Error::ZeroCustomerId { index });
        }
        if !seen.insert(c.id) {
            return Err(Error::DuplicateCustomerId { index, id: c.id });
        }
    }
    let mut arrivals: Vec<usize> = (0..customers.len()).collect();
    arrivals.sort_by(|&a, &b| {
        customers[a]
            .arrival
            .cmp(&customers[b].arrival)
            .then(customers[b].id.cmp(&customers[a].id))
    });
    let mut openings: Vec<usize> = (0..servers.len()).collect();
    openings.sort_by(|&a, &b| {
        servers[a]
            .opens
            .cmp(&servers[b].opens)
            .then(servers[b].id.cmp(&servers[a].id))
    });
    let mut opened = vec![false; servers.len()];
    let mut idle_since = vec![0; servers.len()];
    let mut active: Vec<Option<(usize, u64)>> = vec![None; servers.len()];
    let (mut na, mut no) = (0, 0);
    let mut queue = VecDeque::new();
    let mut events = Vec::new();
    while na < arrivals.len()
        || no < openings.len()
        || active.iter().any(Option::is_some)
        || !queue.is_empty()
    {
        let time = arrivals
            .get(na)
            .map(|&i| customers[i].arrival)
            .into_iter()
            .chain(openings.get(no).map(|&i| servers[i].opens))
            .chain(active.iter().filter_map(|a| a.map(|(_, t)| t)))
            .min()
            .unwrap();
        let mut done: Vec<usize> = (0..servers.len())
            .filter(|&s| active[s].is_some_and(|(_, t)| t == time))
            .collect();
        done.sort_by_key(|&s| servers[s].id);
        for s in done {
            let (i, _) = active[s].take().unwrap();
            idle_since[s] = time;
            events.push(Event::Finish {
                time,
                customer: customers[i].id,
                server: servers[s].id,
            });
        }
        while no < openings.len() && servers[openings[no]].opens == time {
            let s = openings[no];
            no += 1;
            opened[s] = true;
            idle_since[s] = time;
            events.push(Event::Open {
                time,
                server: servers[s].id,
            });
        }
        while na < arrivals.len() && customers[arrivals[na]].arrival == time {
            let i = arrivals[na];
            na += 1;
            queue.push_back(i);
            events.push(Event::Arrive {
                time,
                customer: customers[i].id,
            });
        }
        while !queue.is_empty() {
            let selected = (0..servers.len())
                .filter(|&s| opened[s] && active[s].is_none())
                .min_by(|&a, &b| {
                    idle_since[a]
                        .cmp(&idle_since[b])
                        .then(servers[a].overhead.cmp(&servers[b].overhead))
                        .then(servers[b].id.cmp(&servers[a].id))
                });
            let Some(s) = selected else {
                break;
            };
            let i = queue.pop_front().unwrap();
            let c = &customers[i];
            let finish = time
                .checked_add(c.service)
                .and_then(|t| {
                    t.checked_add(if c.service == 0 {
                        0
                    } else {
                        servers[s].overhead
                    })
                })
                .ok_or(Error::TimeOverflow {
                    customer: c.id,
                    server: servers[s].id,
                    time,
                })?;
            events.push(Event::Start {
                time,
                customer: c.id,
                server: servers[s].id,
                finish,
            });
            if finish == time {
                events.push(Event::Finish {
                    time,
                    customer: c.id,
                    server: servers[s].id,
                });
                idle_since[s] = time;
            } else {
                active[s] = Some((i, finish));
            }
        }
    }
    Ok(events)
}
