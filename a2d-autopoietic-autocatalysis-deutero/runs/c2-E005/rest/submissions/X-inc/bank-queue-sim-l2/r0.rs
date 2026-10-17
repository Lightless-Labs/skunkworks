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
    busy: Option<(u64, u32)>, // (finish time, customer id)
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

    let mut arrivals: Vec<Customer> = customers.to_vec();
    arrivals.sort_by(|a, b| a.arrival.cmp(&b.arrival).then_with(|| b.id.cmp(&a.id)));

    let mut states: Vec<ServerState> = servers
        .iter()
        .cloned()
        .map(|server| ServerState {
            idle_since: server.opens,
            opened: false,
            server,
            busy: None,
        })
        .collect();

    let mut events = Vec::new();
    let mut waiting: Vec<usize> = Vec::new();
    let mut arrival_index = 0;
    let mut time = 0u64;

    loop {
        // Completion phase, ordered by ascending server ID.
        let mut finishing: Vec<usize> = states
            .iter()
            .enumerate()
            .filter_map(|(i, state)| {
                state
                    .busy
                    .filter(|(finish, _)| *finish == time)
                    .map(|_| i)
            })
            .collect();
        finishing.sort_by_key(|&i| states[i].server.id);
        for i in finishing {
            let (finish, customer) = states[i].busy.take().unwrap();
            events.push(Event::Finish {
                time: finish,
                customer,
                server: states[i].server.id,
            });
            states[i].idle_since = time;
        }

        // Opening phase, ordered by descending server ID.
        let mut opening: Vec<usize> = states
            .iter()
            .enumerate()
            .filter_map(|(i, state)| {
                (!state.opened && state.server.opens == time).then_some(i)
            })
            .collect();
        opening.sort_by(|&a, &b| states[b].server.id.cmp(&states[a].server.id));
        for i in opening {
            states[i].opened = true;
            events.push(Event::Open {
                time,
                server: states[i].server.id,
            });
        }

        // Arrival phase, already sorted by time and descending ID.
        while arrival_index < arrivals.len() && arrivals[arrival_index].arrival == time {
            let customer = &arrivals[arrival_index];
            events.push(Event::Arrive {
                time,
                customer: customer.id,
            });
            waiting.push(arrival_index);
            arrival_index += 1;
        }

        // Dispatch repeatedly, recalculating the best eligible server each time.
        while !waiting.is_empty() {
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
                .map(|(i, _)| i);

            let Some(server_index) = selected else {
                break;
            };

            let customer_index = waiting.remove(0);
            let customer = &arrivals[customer_index];
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
            } else {
                states[server_index].busy = Some((finish, customer.id));
            }
        }

        if arrival_index == arrivals.len()
            && states.iter().all(|state| state.opened && state.busy.is_none())
            && waiting.is_empty()
        {
            break;
        }

        let next_arrival = arrivals.get(arrival_index).map(|c| c.arrival);
        let next_open = states
            .iter()
            .filter(|state| !state.opened)
            .map(|state| state.server.opens)
            .min();
        let next_finish = states
            .iter()
            .filter_map(|state| state.busy.map(|(finish, _)| finish))
            .min();

        let next_time = [next_arrival, next_open, next_finish]
            .into_iter()
            .flatten()
            .filter(|&t| t > time)
            .min();

        match next_time {
            Some(next) => time = next,
            None => break,
        }
    }

    Ok(events)
}