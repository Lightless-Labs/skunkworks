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

    for (index, server) in servers.iter().enumerate() {
        if server.id == 0 {
            return Err(Error::ZeroServerId { index });
        }
        if servers[..index].iter().any(|previous| previous.id == server.id) {
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
            .any(|previous| previous.id == customer.id)
        {
            return Err(Error::DuplicateCustomerId {
                index,
                id: customer.id,
            });
        }
    }

    let mut sorted_customers = customers.to_vec();
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
    let mut waiting: Vec<Customer> = Vec::new();
    let mut customer_index = 0usize;
    let mut now = 0u64;

    loop {
        let mut finished: Vec<(u32, usize)> = states
            .iter()
            .enumerate()
            .filter_map(|(index, state)| {
                (state.busy_until == Some(now)).then_some((state.server.id, index))
            })
            .collect();
        finished.sort_by_key(|&(id, _)| id);

        for (_, index) in finished {
            let state = &mut states[index];
            let customer = state.busy_customer.take().unwrap();
            state.busy_until = None;
            state.idle_since = now;
            events.push(Event::Finish {
                time: now,
                customer,
                server: state.server.id,
            });
        }

        let mut opening: Vec<usize> = states
            .iter()
            .enumerate()
            .filter_map(|(index, state)| {
                (!state.opened && state.server.opens == now).then_some(index)
            })
            .collect();
        opening.sort_by_key(|&index| std::cmp::Reverse(states[index].server.id));

        for index in opening {
            states[index].opened = true;
            events.push(Event::Open {
                time: now,
                server: states[index].server.id,
            });
        }

        let start_index = customer_index;
        while customer_index < sorted_customers.len()
            && sorted_customers[customer_index].arrival == now
        {
            customer_index += 1;
        }
        let mut arrivals = sorted_customers[start_index..customer_index].to_vec();
        arrivals.sort_by_key(|customer| std::cmp::Reverse(customer.id));
        for customer in arrivals {
            events.push(Event::Arrive {
                time: now,
                customer: customer.id,
            });
            waiting.push(customer);
        }

        while !waiting.is_empty() {
            let selected = states
                .iter()
                .enumerate()
                .filter(|(_, state)| {
                    state.opened && state.busy_until.is_none()
                })
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

            let customer = waiting.remove(0);
            let server = &states[server_index].server;
            let finish = customer
                .service
                .checked_add(now)
                .and_then(|time| time.checked_add(server.overhead))
                .ok_or(Error::TimeOverflow {
                    customer: customer.id,
                    server: server.id,
                    time: now,
                })?;

            events.push(Event::Start {
                time: now,
                customer: customer.id,
                server: server.id,
                finish,
            });

            if finish == now {
                events.push(Event::Finish {
                    time: now,
                    customer: customer.id,
                    server: server.id,
                });
                states[server_index].idle_since = now;
            } else {
                states[server_index].busy_until = Some(finish);
                states[server_index].busy_customer = Some(customer.id);
            }
        }

        if customer_index == sorted_customers.len()
            && waiting.is_empty()
            && states.iter().all(|state| state.busy_until.is_none())
            && states.iter().all(|state| state.opened)
        {
            break;
        }

        let next_arrival = sorted_customers
            .get(customer_index)
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
            .filter(|&time| time > now)
            .min();

        match next_time {
            Some(time) => now = time,
            None => break,
        }
    }

    Ok(events)
}

fn main() {}