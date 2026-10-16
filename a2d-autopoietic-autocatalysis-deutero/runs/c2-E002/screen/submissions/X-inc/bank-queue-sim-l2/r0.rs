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
    occupied_by: Option<u32>,
    finish_at: Option<u64>,
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

    let mut arrivals: Vec<&Customer> = customers.iter().collect();
    arrivals.sort_by(|a, b| a.arrival.cmp(&b.arrival).then_with(|| b.id.cmp(&a.id)));

    let mut states: Vec<ServerState> = servers
        .iter()
        .cloned()
        .map(|server| ServerState {
            idle_since: server.opens,
            server,
            opened: false,
            occupied_by: None,
            finish_at: None,
        })
        .collect();

    let mut events = Vec::new();
    let mut waiting: Vec<&Customer> = Vec::new();
    let mut next_arrival = 0usize;

    loop {
        let mut next_time: Option<u64> = None;
        let mut consider = |time: u64| {
            next_time = Some(next_time.map_or(time, |current| current.min(time)));
        };

        if next_arrival < arrivals.len() {
            consider(arrivals[next_arrival].arrival);
        }
        for state in &states {
            if !state.opened {
                consider(state.server.opens);
            }
            if let Some(time) = state.finish_at {
                consider(time);
            }
        }

        let Some(time) = next_time else {
            break;
        };

        let mut finishing: Vec<usize> = states
            .iter()
            .enumerate()
            .filter_map(|(index, state)| (state.finish_at == Some(time)).then_some(index))
            .collect();
        finishing.sort_by_key(|&index| states[index].server.id);

        for index in finishing {
            let state = &mut states[index];
            let customer = state
                .occupied_by
                .take()
                .expect("a scheduled finish must have an occupying customer");
            state.finish_at = None;
            state.idle_since = time;
            events.push(Event::Finish {
                time,
                customer,
                server: state.server.id,
            });
        }

        let mut opening: Vec<usize> = states
            .iter()
            .enumerate()
            .filter_map(|(index, state)| {
                (!state.opened && state.server.opens == time).then_some(index)
            })
            .collect();
        opening.sort_by_key(|&index| std::cmp::Reverse(states[index].server.id));

        for index in opening {
            states[index].opened = true;
            events.push(Event::Open {
                time,
                server: states[index].server.id,
            });
        }

        while next_arrival < arrivals.len() && arrivals[next_arrival].arrival == time {
            let customer = arrivals[next_arrival];
            events.push(Event::Arrive {
                time,
                customer: customer.id,
            });
            waiting.push(customer);
            next_arrival += 1;
        }

        while !waiting.is_empty() {
            let selected = states
                .iter()
                .enumerate()
                .filter(|(_, state)| {
                    state.opened && state.occupied_by.is_none()
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

            let customer = waiting[0];
            let server = &states[server_index].server;

            let finish = customer
                .service
                .checked_add(time)
                .and_then(|finish| finish.checked_add(server.overhead))
                .ok_or(Error::TimeOverflow {
                    customer: customer.id,
                    server: server.id,
                    time,
                })?;

            waiting.remove(0);
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
                states[server_index].occupied_by = Some(customer.id);
                states[server_index].finish_at = Some(finish);
            }
        }
    }

    Ok(events)
}