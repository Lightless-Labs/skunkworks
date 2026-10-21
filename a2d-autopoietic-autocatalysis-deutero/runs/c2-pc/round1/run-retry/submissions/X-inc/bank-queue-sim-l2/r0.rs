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
    id: u32,
    opens: u64,
    overhead: u64,
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

    let mut seen_servers = std::collections::HashSet::new();
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

    let mut seen_customers = std::collections::HashSet::new();
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

    let mut arrivals: Vec<&Customer> = customers.iter().collect();
    arrivals.sort_by(|a, b| a.arrival.cmp(&b.arrival).then_with(|| b.id.cmp(&a.id)));

    let mut states: Vec<ServerState> = servers
        .iter()
        .map(|server| ServerState {
            id: server.id,
            opens: server.opens,
            overhead: server.overhead,
            opened: false,
            busy: None,
            idle_since: server.opens,
        })
        .collect();

    let mut events = Vec::new();
    let mut waiting: std::collections::VecDeque<&Customer> = std::collections::VecDeque::new();
    let mut arrival_index = 0usize;
    let mut now = 0u64;

    loop {
        let mut finishing: Vec<usize> = states
            .iter()
            .enumerate()
            .filter_map(|(i, state)| {
                state
                    .busy
                    .filter(|(finish, _)| *finish == now)
                    .map(|_| i)
            })
            .collect();
        finishing.sort_by_key(|&i| states[i].id);
        for i in finishing {
            let (time, customer) = states[i].busy.take().unwrap();
            events.push(Event::Finish {
                time,
                customer,
                server: states[i].id,
            });
            states[i].idle_since = now;
        }

        let mut opening: Vec<usize> = states
            .iter()
            .enumerate()
            .filter_map(|(i, state)| (!state.opened && state.opens == now).then_some(i))
            .collect();
        opening.sort_by_key(|&i| std::cmp::Reverse(states[i].id));
        for i in opening {
            states[i].opened = true;
            events.push(Event::Open {
                time: now,
                server: states[i].id,
            });
        }

        while arrival_index < arrivals.len() && arrivals[arrival_index].arrival == now {
            let customer = arrivals[arrival_index];
            events.push(Event::Arrive {
                time: now,
                customer: customer.id,
            });
            waiting.push_back(customer);
            arrival_index += 1;
        }

        while let Some(customer) = waiting.front().copied() {
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
                .map(|(i, _)| i);

            let Some(i) = selected else {
                break;
            };

            let server_id = states[i].id;
            let finish = now
                .checked_add(customer.service)
                .and_then(|time_after_service| time_after_service.checked_add(states[i].overhead))
                .ok_or(Error::TimeOverflow {
                    customer: customer.id,
                    server: server_id,
                    time: now,
                })?;

            waiting.pop_front();
            events.push(Event::Start {
                time: now,
                customer: customer.id,
                server: server_id,
                finish,
            });

            if finish == now {
                events.push(Event::Finish {
                    time: now,
                    customer: customer.id,
                    server: server_id,
                });
                states[i].idle_since = now;
            } else {
                states[i].busy = Some((finish, customer.id));
            }
        }

        if arrival_index == arrivals.len()
            && waiting.is_empty()
            && states.iter().all(|state| state.opened && state.busy.is_none())
        {
            break;
        }

        let next_arrival = arrivals.get(arrival_index).map(|c| c.arrival);
        let next_open = states
            .iter()
            .filter(|state| !state.opened)
            .map(|state| state.opens)
            .min();
        let next_finish = states
            .iter()
            .filter_map(|state| state.busy.map(|(finish, _)| finish))
            .min();

        let next = [next_arrival, next_open, next_finish]
            .into_iter()
            .flatten()
            .filter(|&time| time > now)
            .min();

        match next {
            Some(time) => now = time,
            None => break,
        }
    }

    Ok(events)
}