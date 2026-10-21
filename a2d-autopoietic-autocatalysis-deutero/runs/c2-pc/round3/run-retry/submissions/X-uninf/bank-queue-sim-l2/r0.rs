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

#[derive(Clone)]
struct ServerState {
    server: Server,
    opened: bool,
    idle_since: u64,
    busy_until: Option<u64>,
    busy_customer: Option<u32>,
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

    let mut seen_servers = HashSet::with_capacity(servers.len());
    for (index, server) in servers.iter().enumerate() {
        if server.id == 0 {
            return Err(Error::ZeroServerId { index });
        }
        if !seen_servers.insert(server.id) {
            return Err(Error::DuplicateServerId {
                index,
                id: server.id,
            });
        }
    }

    let mut seen_customers = HashSet::with_capacity(customers.len());
    for (index, customer) in customers.iter().enumerate() {
        if customer.id == 0 {
            return Err(Error::ZeroCustomerId { index });
        }
        if !seen_customers.insert(customer.id) {
            return Err(Error::DuplicateCustomerId {
                index,
                id: customer.id,
            });
        }
    }

    let mut sorted_customers: Vec<Customer> = customers.to_vec();
    sorted_customers.sort_by(|a, b| {
        a.arrival
            .cmp(&b.arrival)
            .then_with(|| b.id.cmp(&a.id))
    });

    let mut states: Vec<ServerState> = servers
        .iter()
        .cloned()
        .map(|server| ServerState {
            idle_since: server.opens,
            server,
            opened: false,
            busy_until: None,
            busy_customer: None,
        })
        .collect();

    let mut events = Vec::new();
    let mut waiting: VecDeque<usize> = VecDeque::new();
    let mut next_customer = 0usize;
    let mut time = 0u64;

    loop {
        let mut finishing: Vec<usize> = states
            .iter()
            .enumerate()
            .filter_map(|(i, state)| (state.busy_until == Some(time)).then_some(i))
            .collect();
        finishing.sort_by_key(|&i| states[i].server.id);

        for i in finishing {
            let state = &mut states[i];
            events.push(Event::Finish {
                time,
                customer: state.busy_customer.take().unwrap(),
                server: state.server.id,
            });
            state.busy_until = None;
            state.idle_since = time;
        }

        let mut opening: Vec<usize> = states
            .iter()
            .enumerate()
            .filter_map(|(i, state)| {
                (!state.opened && state.server.opens == time).then_some(i)
            })
            .collect();
        opening.sort_by_key(|&i| std::cmp::Reverse(states[i].server.id));

        for i in opening {
            states[i].opened = true;
            events.push(Event::Open {
                time,
                server: states[i].server.id,
            });
        }

        while next_customer < sorted_customers.len()
            && sorted_customers[next_customer].arrival == time
        {
            let customer = &sorted_customers[next_customer];
            events.push(Event::Arrive {
                time,
                customer: customer.id,
            });
            waiting.push_back(next_customer);
            next_customer += 1;
        }

        while !waiting.is_empty() {
            let selected = states
                .iter()
                .enumerate()
                .filter(|(_, state)| state.opened && state.busy_until.is_none())
                .min_by(|(_, a), (_, b)| {
                    a.idle_since
                        .cmp(&b.idle_since)
                        .then_with(|| a.server.overhead.cmp(&b.server.overhead))
                        .then_with(|| b.server.id.cmp(&a.server.id))
                })
                .map(|(i, _)| i);

            let Some(server_index) = selected else {
                break;
            };

            let customer_index = waiting.pop_front().unwrap();
            let customer = &sorted_customers[customer_index];
            let server = &states[server_index].server;

            let finish = time
                .checked_add(customer.service)
                .and_then(|t| t.checked_add(server.overhead))
                .ok_or(Error::TimeOverflow {
                    customer: customer.id,
                    server: server.id,
                    time,
                })?;

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
            } else {
                states[server_index].busy_until = Some(finish);
                states[server_index].busy_customer = Some(customer.id);
            }
        }

        if next_customer == sorted_customers.len()
            && waiting.is_empty()
            && states.iter().all(|state| state.busy_until.is_none())
            && states.iter().all(|state| state.opened)
        {
            break;
        }

        let next_arrival = sorted_customers
            .get(next_customer)
            .map(|customer| customer.arrival);
        let next_open = states
            .iter()
            .filter(|state| !state.opened)
            .map(|state| state.server.opens)
            .min();
        let next_finish = states.iter().filter_map(|state| state.busy_until).min();

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