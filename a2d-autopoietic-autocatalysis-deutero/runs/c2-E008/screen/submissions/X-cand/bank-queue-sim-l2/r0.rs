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
        if customers[..index]
            .iter()
            .any(|prior| prior.id == customer.id)
        {
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
            idle_since: server.opens,
            busy: None,
        })
        .collect();

    let mut events = Vec::new();
    let mut waiting: Vec<&Customer> = Vec::new();
    let mut arrival_index = 0usize;
    let mut now = 0u64;

    loop {
        let next_finish = states
            .iter()
            .filter_map(|server| server.busy.map(|(finish, _)| finish))
            .min();
        let next_open = states
            .iter()
            .filter(|server| !server.opened)
            .map(|server| server.opens)
            .min();
        let next_arrival = arrivals.get(arrival_index).map(|customer| customer.arrival);

        let Some(next_time) = [next_finish, next_open, next_arrival]
            .into_iter()
            .flatten()
            .min()
        else {
            break;
        };
        now = next_time;

        let mut finishing: Vec<(u32, usize, u32)> = states
            .iter()
            .enumerate()
            .filter_map(|(index, server)| {
                server
                    .busy
                    .filter(|(finish, _)| *finish == now)
                    .map(|(_, customer)| (server.id, index, customer))
            })
            .collect();
        finishing.sort_by_key(|entry| entry.0);
        for (server_id, index, customer_id) in finishing {
            states[index].busy = None;
            states[index].idle_since = now;
            events.push(Event::Finish {
                time: now,
                customer: customer_id,
                server: server_id,
            });
        }

        let mut opening: Vec<(u32, usize)> = states
            .iter()
            .enumerate()
            .filter(|(_, server)| !server.opened && server.opens == now)
            .map(|(index, server)| (server.id, index))
            .collect();
        opening.sort_by(|a, b| b.0.cmp(&a.0));
        for (server_id, index) in opening {
            states[index].opened = true;
            events.push(Event::Open {
                time: now,
                server: server_id,
            });
        }

        let cohort_start = arrival_index;
        while arrival_index < arrivals.len() && arrivals[arrival_index].arrival == now {
            arrival_index += 1;
        }
        for customer in &arrivals[cohort_start..arrival_index] {
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
                .filter(|(_, server)| server.opened && server.busy.is_none())
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

            let customer = waiting.remove(0);
            let server_id = states[server_index].id;
            let overhead = states[server_index].overhead;

            let finish = now
                .checked_add(customer.service)
                .and_then(|after_service| after_service.checked_add(overhead))
                .ok_or(Error::TimeOverflow {
                    customer: customer.id,
                    server: server_id,
                    time: now,
                })?;

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
                states[server_index].idle_since = now;
            } else {
                states[server_index].busy = Some((finish, customer.id));
            }
        }
    }

    Ok(events)
}