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
    id: u32,
    opens: u64,
    overhead: u64,
    opened: bool,
    busy_until: Option<u64>,
    active_customer: Option<u32>,
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
        if servers[..index].iter().any(|s| s.id == server.id) {
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
        if customers[..index].iter().any(|c| c.id == customer.id) {
            return Err(Error::DuplicateCustomerId {
                index,
                id: customer.id,
            });
        }
    }

    let mut arrivals: Vec<&Customer> = customers.iter().collect();
    arrivals.sort_by(|a, b| {
        a.arrival
            .cmp(&b.arrival)
            .then_with(|| b.id.cmp(&a.id))
    });

    let mut states: Vec<ServerState> = servers
        .iter()
        .map(|server| ServerState {
            id: server.id,
            opens: server.opens,
            overhead: server.overhead,
            opened: false,
            busy_until: None,
            active_customer: None,
            idle_since: server.opens,
        })
        .collect();

    let mut events = Vec::new();
    let mut waiting: Vec<&Customer> = Vec::new();
    let mut arrival_index = 0usize;
    let mut time = 0u64;

    loop {
        let mut finishing: Vec<usize> = states
            .iter()
            .enumerate()
            .filter_map(|(i, state)| (state.busy_until == Some(time)).then_some(i))
            .collect();
        finishing.sort_by_key(|&i| states[i].id);

        for i in finishing {
            let customer = states[i].active_customer.take().unwrap();
            events.push(Event::Finish {
                time,
                customer,
                server: states[i].id,
            });
            states[i].busy_until = None;
            states[i].idle_since = time;
        }

        let mut opening: Vec<usize> = states
            .iter()
            .enumerate()
            .filter_map(|(i, state)| {
                (!state.opened && state.opens == time).then_some(i)
            })
            .collect();
        opening.sort_by_key(|&i| std::cmp::Reverse(states[i].id));

        for i in opening {
            states[i].opened = true;
            events.push(Event::Open {
                time,
                server: states[i].id,
            });
        }

        while arrival_index < arrivals.len() && arrivals[arrival_index].arrival == time {
            let customer = arrivals[arrival_index];
            events.push(Event::Arrive {
                time,
                customer: customer.id,
            });
            waiting.push(customer);
            arrival_index += 1;
        }

        while !waiting.is_empty() {
            let candidate = states
                .iter()
                .enumerate()
                .filter(|(_, state)| state.opened && state.busy_until.is_none())
                .min_by(|(_, a), (_, b)| {
                    a.idle_since
                        .cmp(&b.idle_since)
                        .then_with(|| a.overhead.cmp(&b.overhead))
                        .then_with(|| b.id.cmp(&a.id))
                })
                .map(|(i, _)| i);

            let Some(server_index) = candidate else {
                break;
            };

            let customer = waiting.remove(0);
            let server_id = states[server_index].id;
            let finish = customer
                .service
                .checked_add(time)
                .and_then(|t| t.checked_add(states[server_index].overhead))
                .ok_or(Error::TimeOverflow {
                    customer: customer.id,
                    server: server_id,
                    time,
                })?;

            events.push(Event::Start {
                time,
                customer: customer.id,
                server: server_id,
                finish,
            });

            if finish == time {
                events.push(Event::Finish {
                    time,
                    customer: customer.id,
                    server: server_id,
                });
                states[server_index].idle_since = time;
            } else {
                states[server_index].busy_until = Some(finish);
                states[server_index].active_customer = Some(customer.id);
            }
        }

        if arrival_index == arrivals.len()
            && states.iter().all(|state| state.opened)
            && states.iter().all(|state| state.busy_until.is_none())
            && waiting.is_empty()
        {
            break;
        }

        let next_arrival = arrivals.get(arrival_index).map(|customer| customer.arrival);
        let next_open = states
            .iter()
            .filter(|state| !state.opened)
            .map(|state| state.opens)
            .min();
        let next_finish = states.iter().filter_map(|state| state.busy_until).min();

        let next = [next_arrival, next_open, next_finish]
            .into_iter()
            .flatten()
            .filter(|&t| t > time)
            .min();

        match next {
            Some(next_time) => time = next_time,
            None => break,
        }
    }

    Ok(events)
}