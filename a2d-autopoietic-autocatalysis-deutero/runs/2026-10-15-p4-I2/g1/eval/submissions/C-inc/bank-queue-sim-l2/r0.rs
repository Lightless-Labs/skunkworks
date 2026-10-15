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
    Open { time: u64, server: u32 },
    Arrive { time: u64, customer: u32 },
    Start {
        time: u64,
        customer: u32,
        server: u32,
        finish: u64,
    },
    Finish { time: u64, customer: u32, server: u32 },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Error {
    NoServers,
    TooManyServers { count: usize },
    ZeroServerId { index: usize },
    DuplicateServerId { index: usize, id: u32 },
    ZeroCustomerId { index: usize },
    DuplicateCustomerId { index: usize, id: u32 },
    TimeOverflow {
        customer: u32,
        server: u32,
        time: u64,
    },
}

#[derive(Clone)]
struct ServerState {
    server: Server,
    opened: bool,
    idle_since: u64,
    occupied_until: Option<(u64, u32)>,
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

    for (index, server) in servers.iter().enumerate() {
        if server.id == 0 {
            return Err(Error::ZeroServerId { index });
        }
        if servers[..index].iter().any(|earlier| earlier.id == server.id) {
            return Err(Error::DuplicateServerId {
                index,
                id: server.id,
            });
        }
    }

    for (index, customer) in customers.iter().enumerate() {
        if customer.id == 0 {
            return Err(Error::ZeroCustomerId { index });
        }
        if customers[..index]
            .iter()
            .any(|earlier| earlier.id == customer.id)
        {
            return Err(Error::DuplicateCustomerId {
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

    let mut states: Vec<ServerState> = servers
        .iter()
        .cloned()
        .map(|server| ServerState {
            idle_since: server.opens,
            opened: false,
            occupied_until: None,
            server,
        })
        .collect();

    let mut events = Vec::new();
    let mut waiting: VecDeque<usize> = VecDeque::new();
    let mut arrival_pos = 0usize;
    let mut finished_count = 0usize;
    let mut time = 0u64;

    loop {
        let mut due_finishes: Vec<(u32, usize, u32)> = states
            .iter()
            .enumerate()
            .filter_map(|(index, state)| {
                state.occupied_until.and_then(|(finish, customer)| {
                    if finish == time {
                        Some((state.server.id, index, customer))
                    } else {
                        None
                    }
                })
            })
            .collect();
        due_finishes.sort_by_key(|&(id, _, _)| id);
        for (_, index, customer) in due_finishes {
            let state = &mut states[index];
            state.occupied_until = None;
            state.idle_since = time;
            events.push(Event::Finish {
                time,
                customer,
                server: state.server.id,
            });
            finished_count += 1;
        }

        let mut due_opens: Vec<usize> = states
            .iter()
            .enumerate()
            .filter_map(|(index, state)| {
                (!state.opened && state.server.opens == time).then_some(index)
            })
            .collect();
        due_opens.sort_by(|&a, &b| states[b].server.id.cmp(&states[a].server.id));
        for index in due_opens {
            states[index].opened = true;
            events.push(Event::Open {
                time,
                server: states[index].server.id,
            });
        }

        let start = arrival_pos;
        while arrival_pos < arrivals.len() && customers[arrivals[arrival_pos]].arrival == time {
            arrival_pos += 1;
        }
        for &index in &arrivals[start..arrival_pos] {
            let customer = &customers[index];
            events.push(Event::Arrive {
                time,
                customer: customer.id,
            });
            waiting.push_back(index);
        }

        while let Some(&customer_index) = waiting.front() {
            let selected = states
                .iter()
                .enumerate()
                .filter(|(_, state)| state.opened && state.occupied_until.is_none())
                .min_by(|(_, a), (_, b)| {
                    a.idle_since
                        .cmp(&b.idle_since)
                        .then_with(|| a.server.overhead.cmp(&b.server.overhead))
                        .then_with(|| b.server.id.cmp(&a.server.id))
                })
                .map(|(index, _)| index);

            let Some(server_index) = selected else {
                break;
            };

            let customer = &customers[customer_index];
            let server = &states[server_index].server;
            let finish = time
                .checked_add(customer.service)
                .and_then(|value| value.checked_add(server.overhead))
                .ok_or(Error::TimeOverflow {
                    customer: customer.id,
                    server: server.id,
                    time,
                })?;

            waiting.pop_front();
            events.push(Event::Start {
                time,
                customer: customer.id,
                server: server.id,
                finish,
            });

            if finish == time {
                events.push(Event::Finish {
                    time,
                    customer: customer.id,
                    server: server.id,
                });
                states[server_index].idle_since = time;
                finished_count += 1;
            } else {
                states[server_index].occupied_until = Some((finish, customer.id));
            }
        }

        if finished_count == customers.len()
            && arrival_pos == arrivals.len()
            && waiting.is_empty()
        {
            for state in &states {
                if !state.opened {
                    // Unopened servers still emit Open at their scheduled time.
                    // Continue advancing until all have opened.
                    break;
                }
            }
            if states.iter().all(|state| state.opened) {
                break;
            }
        }

        let next_arrival = arrivals
            .get(arrival_pos)
            .map(|&index| customers[index].arrival);
        let next_open = states
            .iter()
            .filter(|state| !state.opened)
            .map(|state| state.server.opens)
            .min();
        let next_finish = states
            .iter()
            .filter_map(|state| state.occupied_until.map(|(finish, _)| finish))
            .min();

        let next_time = [next_arrival, next_open, next_finish]
            .into_iter()
            .flatten()
            .filter(|&candidate| candidate > time)
            .min();

        match next_time {
            Some(next) => time = next,
            None => break,
        }
    }

    Ok(events)
}

use std::collections::VecDeque;

fn main() {}