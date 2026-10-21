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
    busy: Option<(u64, u32)>, // finish time, customer ID
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

    let mut server_ids = std::collections::HashSet::new();
    for (index, server) in servers.iter().enumerate() {
        if server.id == 0 {
            return Err(Error::ZeroServerId { index });
        }
        if !server_ids.insert(server.id) {
            return Err(Error::DuplicateServerId {
                index,
                id: server.id,
            });
        }
    }

    let mut customer_ids = std::collections::HashSet::new();
    for (index, customer) in customers.iter().enumerate() {
        if customer.id == 0 {
            return Err(Error::ZeroCustomerId { index });
        }
        if !customer_ids.insert(customer.id) {
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
            busy: None,
        })
        .collect();

    let mut events = Vec::new();
    let mut waiting: std::collections::VecDeque<Customer> =
        std::collections::VecDeque::new();
    let mut customer_index = 0usize;
    let mut now = 0u64;

    loop {
        // Finish events, ordered by ascending server ID.
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
        finishing.sort_by_key(|&i| states[i].server.id);
        for i in finishing {
            let (time, customer) = states[i].busy.take().unwrap();
            events.push(Event::Finish {
                time,
                customer,
                server: states[i].server.id,
            });
            states[i].idle_since = now;
        }

        // Open events, ordered by descending server ID.
        let mut opening: Vec<usize> = states
            .iter()
            .enumerate()
            .filter_map(|(i, state)| {
                (!state.opened && state.server.opens == now).then_some(i)
            })
            .collect();
        opening.sort_by_key(|&i| std::cmp::Reverse(states[i].server.id));
        for i in opening {
            states[i].opened = true;
            events.push(Event::Open {
                time: now,
                server: states[i].server.id,
            });
        }

        // Arrivals, already sorted by time ascending and cohort ID descending.
        while customer_index < ordered_customers.len()
            && ordered_customers[customer_index].arrival == now
        {
            let customer = ordered_customers[customer_index].clone();
            events.push(Event::Arrive {
                time: now,
                customer: customer.id,
            });
            waiting.push_back(customer);
            customer_index += 1;
        }

        // Dispatch FIFO customers to the best-ranked eligible servers.
        loop {
            let Some(customer) = waiting.front() else {
                break;
            };

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

            let customer = waiting.front().unwrap();
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

            let customer = waiting.pop_front().unwrap();
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
                states[server_index].busy = Some((finish, customer.id));
            }
        }

        if customer_index == ordered_customers.len()
            && waiting.is_empty()
            && states.iter().all(|state| state.busy.is_none())
            && states.iter().all(|state| state.opened)
        {
            break;
        }

        let next_arrival = ordered_customers
            .get(customer_index)
            .map(|customer| customer.arrival);
        let next_open = states
            .iter()
            .filter(|state| !state.opened)
            .map(|state| state.server.opens)
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