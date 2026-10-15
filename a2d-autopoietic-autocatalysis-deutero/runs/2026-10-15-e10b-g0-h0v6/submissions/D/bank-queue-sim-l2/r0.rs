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
    id: u32,
    opens: u64,
    overhead: u64,
    opened: bool,
    idle_since: u64,
    busy: Option<(u64, u32)>,
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

    let mut customer_order: Vec<usize> = (0..customers.len()).collect();
    customer_order.sort_by(|&a, &b| {
        customers[a]
            .arrival
            .cmp(&customers[b].arrival)
            .then_with(|| customers[b].id.cmp(&customers[a].id))
    });

    let mut states: Vec<ServerState> = servers
        .iter()
        .map(|server| ServerState {
            id: server.id,
            opens: server.opens,
            overhead: server.overhead,
            opened: false,
            idle_since: server.opens,
            busy: None,
        })
        .collect();

    let mut events = Vec::new();
    let mut waiting: Vec<usize> = Vec::new();
    let mut arrival_index = 0usize;
    let mut finished_customers = 0usize;
    let mut time = 0u64;

    loop {
        let mut finishing: Vec<usize> = states
            .iter()
            .enumerate()
            .filter_map(|(index, state)| {
                state
                    .busy
                    .filter(|(finish, _)| *finish == time)
                    .map(|_| index)
            })
            .collect();
        finishing.sort_by_key(|&index| states[index].id);

        for index in finishing {
            let (_, customer) = states[index].busy.take().expect("selected busy server");
            events.push(Event::Finish {
                time,
                customer,
                server: states[index].id,
            });
            states[index].idle_since = time;
            finished_customers += 1;
        }

        let mut opening: Vec<usize> = states
            .iter()
            .enumerate()
            .filter_map(|(index, state)| {
                (!state.opened && state.opens == time).then_some(index)
            })
            .collect();
        opening.sort_by(|&a, &b| states[b].id.cmp(&states[a].id));

        for index in opening {
            states[index].opened = true;
            events.push(Event::Open {
                time,
                server: states[index].id,
            });
        }

        while arrival_index < customer_order.len()
            && customers[customer_order[arrival_index]].arrival == time
        {
            let customer_index = customer_order[arrival_index];
            events.push(Event::Arrive {
                time,
                customer: customers[customer_index].id,
            });
            waiting.push(customer_index);
            arrival_index += 1;
        }

        while !waiting.is_empty() {
            let selected = states
                .iter()
                .enumerate()
                .filter(|(_, state)| state.opened && state.busy.is_none())
                .min_by(|(_, a), (_, b)| {
                    a.idle_since
                        .cmp(&b.idle_since)
                        .then_with(|| a.overhead.cmp(&b.overhead))
                        .then_with(|| b.id.cmp(&a.id))
                })
                .map(|(index, _)| index);

            let Some(server_index) = selected else {
                break;
            };

            let customer_index = waiting.remove(0);
            let customer = &customers[customer_index];
            let server = &states[server_index];

            let after_service =
                time.checked_add(customer.service)
                    .ok_or(Error::TimeOverflow {
                        customer: customer.id,
                        server: server.id,
                        time,
                    })?;
            let finish = after_service
                .checked_add(server.overhead)
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
                finished_customers += 1;
            } else {
                states[server_index].busy = Some((finish, customer.id));
            }
        }

        if finished_customers == customers.len() && states.iter().all(|state| state.opened) {
            break;
        }

        let next_arrival = customer_order
            .get(arrival_index)
            .map(|&index| customers[index].arrival);
        let next_open = states
            .iter()
            .filter(|state| !state.opened)
            .map(|state| state.opens)
            .min();
        let next_finish = states
            .iter()
            .filter_map(|state| state.busy.map(|(finish, _)| finish))
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