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
    TimeOverflow { customer: u32, server: u32, time: u64 },
}

#[derive(Clone)]
struct ServerState {
    server: Server,
    opened: bool,
    idle_since: u64,
    occupied_until: Option<u64>,
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

    let mut ordered_customers: Vec<Customer> = customers.to_vec();
    ordered_customers.sort_by(|a, b| {
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
            occupied_until: None,
        })
        .collect();

    let mut events = Vec::new();
    let mut waiting: Vec<usize> = Vec::new();
    let mut customer_index = 0usize;
    let mut time = 0u64;

    loop {
        let mut finishing: Vec<(u32, usize, u32)> = states
            .iter()
            .enumerate()
            .filter_map(|(i, state)| {
                state
                    .occupied_until
                    .filter(|&finish| finish == time)
                    .map(|_| {
                        let (customer_id, _) = match events.iter().rev().find_map(|event| {
                            if let Event::Start {
                                time: _,
                                customer,
                                server,
                                finish,
                            } = event
                            {
                                if *server == state.server.id && *finish == time {
                                    Some((*customer, *server))
                                } else {
                                    None
                                }
                            } else {
                                None
                            }
                        }) {
                            Some(pair) => pair,
                            None => (0, state.server.id),
                        };
                        (state.server.id, i, customer_id)
                    })
            })
            .collect();
        finishing.sort_by_key(|entry| entry.0);
        for (server_id, i, customer_id) in finishing {
            events.push(Event::Finish {
                time,
                customer: customer_id,
                server: server_id,
            });
            states[i].occupied_until = None;
            states[i].idle_since = time;
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

        let arrival_start = customer_index;
        while customer_index < ordered_customers.len()
            && ordered_customers[customer_index].arrival == time
        {
            customer_index += 1;
        }
        for customer in &ordered_customers[arrival_start..customer_index] {
            events.push(Event::Arrive {
                time,
                customer: customer.id,
            });
            waiting.push(
                customers
                    .iter()
                    .position(|original| original.id == customer.id)
                    .unwrap(),
            );
        }

        loop {
            if waiting.is_empty() {
                break;
            }
            let selected = states
                .iter()
                .enumerate()
                .filter(|(_, state)| {
                    state.opened && state.occupied_until.is_none()
                })
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
            let customer_index_in_input = waiting.remove(0);
            let customer = &customers[customer_index_in_input];
            let server = &states[server_index].server;

            let finish = customer
                .service
                .checked_add(time)
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
                states[server_index].occupied_until = None;
            } else {
                states[server_index].occupied_until = Some(finish);
            }
        }

        let all_customers_done =
            customer_index == ordered_customers.len() && waiting.is_empty();
        let all_servers_open = states.iter().all(|state| state.opened);
        let any_occupied = states.iter().any(|state| state.occupied_until.is_some());

        if all_customers_done && all_servers_open && !any_occupied {
            break;
        }

        let next_arrival = ordered_customers
            .get(customer_index)
            .map(|customer| customer.arrival)
            .filter(|&next| next > time);
        let next_open = states
            .iter()
            .filter(|state| !state.opened && state.server.opens > time)
            .map(|state| state.server.opens)
            .min();
        let next_finish = states
            .iter()
            .filter_map(|state| state.occupied_until)
            .filter(|&finish| finish > time)
            .min();

        let next_time = [next_arrival, next_open, next_finish]
            .into_iter()
            .flatten()
            .min();

        match next_time {
            Some(next) => time = next,
            None => break,
        }
    }

    Ok(events)
}

fn main() {}