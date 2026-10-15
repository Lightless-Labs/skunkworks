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
    occupied: Option<(u64, u32)>, // finish time, customer ID
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
        if servers[..index].iter().any(|prior| prior.id == server.id) {
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
        if customers[..index].iter().any(|prior| prior.id == customer.id) {
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
            server,
            opened: false,
            occupied: None,
        })
        .collect();

    let mut events = Vec::new();
    let mut waiting: Vec<Customer> = Vec::new();
    let mut arrival_index = 0usize;
    let mut finished_customers = 0usize;
    let mut now = 0u64;

    loop {
        let mut finish_indices: Vec<usize> = states
            .iter()
            .enumerate()
            .filter_map(|(i, state)| {
                state
                    .occupied
                    .filter(|(time, _)| *time == now)
                    .map(|_| i)
            })
            .collect();
        finish_indices.sort_by_key(|&i| states[i].server.id);
        for i in finish_indices {
            let (time, customer) = states[i].occupied.take().unwrap();
            events.push(Event::Finish {
                time,
                customer,
                server: states[i].server.id,
            });
            states[i].idle_since = time;
            finished_customers += 1;
        }

        let mut opening_indices: Vec<usize> = states
            .iter()
            .enumerate()
            .filter_map(|(i, state)| (!state.opened && state.server.opens == now).then_some(i))
            .collect();
        opening_indices.sort_by(|&a, &b| states[b].server.id.cmp(&states[a].server.id));
        for i in opening_indices {
            states[i].opened = true;
            events.push(Event::Open {
                time: now,
                server: states[i].server.id,
            });
        }

        let start = arrival_index;
        while arrival_index < arrivals.len() && arrivals[arrival_index].arrival == now {
            arrival_index += 1;
        }
        for customer in &arrivals[start..arrival_index] {
            events.push(Event::Arrive {
                time: now,
                customer: customer.id,
            });
            waiting.push(customer.clone());
        }

        while !waiting.is_empty() {
            let eligible = states
                .iter()
                .enumerate()
                .filter(|(_, state)| state.opened && state.occupied.is_none())
                .min_by(|(_, a), (_, b)| {
                    a.idle_since
                        .cmp(&b.idle_since)
                        .then_with(|| a.server.overhead.cmp(&b.server.overhead))
                        .then_with(|| b.server.id.cmp(&a.server.id))
                })
                .map(|(i, _)| i);

            let Some(server_index) = eligible else {
                break;
            };

            let customer = waiting.remove(0);
            let server = &states[server_index].server;
            let finish = customer
                .service
                .checked_add(now)
                .and_then(|t| t.checked_add(server.overhead))
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
                finished_customers += 1;
            } else {
                states[server_index].occupied = Some((finish, customer.id));
            }
        }

        if finished_customers == customers.len()
            && states.iter().all(|state| state.opened)
        {
            break;
        }

        let next_arrival = arrivals
            .get(arrival_index)
            .map(|customer| customer.arrival);
        let next_open = states
            .iter()
            .filter(|state| !state.opened)
            .map(|state| state.server.opens)
            .min();
        let next_finish = states
            .iter()
            .filter_map(|state| state.occupied.map(|(time, _)| time))
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

fn main() {}