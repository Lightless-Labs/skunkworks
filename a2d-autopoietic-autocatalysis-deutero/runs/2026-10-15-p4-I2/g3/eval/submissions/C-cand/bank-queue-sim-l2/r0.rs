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
    busy: Option<(u64, u32)>,
    idle_since: u64,
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

    let mut ordered_customers = customers.to_vec();
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
            busy: None,
        })
        .collect();

    let mut events = Vec::new();
    let mut waiting = Vec::<Customer>::new();
    let mut next_customer = 0usize;

    loop {
        let mut next_time: Option<u64> = None;

        if let Some(customer) = ordered_customers.get(next_customer) {
            next_time = Some(customer.arrival);
        }
        for state in &states {
            if !state.opened {
                next_time = Some(next_time.map_or(state.server.opens, |t| t.min(state.server.opens)));
            }
            if let Some((finish, _)) = state.busy {
                next_time = Some(next_time.map_or(finish, |t| t.min(finish)));
            }
        }

        let Some(time) = next_time else {
            break;
        };

        let mut finishing: Vec<(u32, usize, u32)> = states
            .iter()
            .enumerate()
            .filter_map(|(index, state)| {
                state
                    .busy
                    .filter(|(finish, _)| *finish == time)
                    .map(|(_, customer)| (state.server.id, index, customer))
            })
            .collect();
        finishing.sort_by_key(|&(server_id, _, _)| server_id);
        for (_, index, customer) in finishing {
            let state = &mut states[index];
            state.busy = None;
            state.idle_since = time;
            events.push(Event::Finish {
                time,
                customer,
                server: state.server.id,
            });
        }

        let mut opening: Vec<(u32, usize)> = states
            .iter()
            .enumerate()
            .filter_map(|(index, state)| {
                (!state.opened && state.server.opens == time)
                    .then_some((state.server.id, index))
            })
            .collect();
        opening.sort_by(|a, b| b.0.cmp(&a.0));
        for (_, index) in opening {
            let state = &mut states[index];
            state.opened = true;
            state.idle_since = time;
            events.push(Event::Open {
                time,
                server: state.server.id,
            });
        }

        while let Some(customer) = ordered_customers.get(next_customer) {
            if customer.arrival != time {
                break;
            }
            waiting.push(customer.clone());
            events.push(Event::Arrive {
                time,
                customer: customer.id,
            });
            next_customer += 1;
        }

        loop {
            let selected = states
                .iter()
                .enumerate()
                .filter(|(_, state)| state.opened && state.busy.is_none())
                .min_by(|(_, a), (_, b)| {
                    a.idle_since
                        .cmp(&b.idle_since)
                        .then_with(|| a.server.overhead.cmp(&b.server.overhead))
                        .then_with(|| b.server.id.cmp(&a.server.id))
                })
                .map(|(index, _)| index);

            let (Some(server_index), Some(customer)) = (selected, waiting.first()) else {
                break;
            };
            let customer = customer.clone();
            waiting.remove(0);
            let server = &states[server_index].server;

            let service_end = time.checked_add(customer.service).ok_or(Error::TimeOverflow {
                customer: customer.id,
                server: server.id,
                time,
            })?;
            let finish = service_end.checked_add(server.overhead).ok_or(Error::TimeOverflow {
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
                states[server_index].busy = Some((finish, customer.id));
            }
        }
    }

    Ok(events)
}

fn main() {}