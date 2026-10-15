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
    occupied: Option<(u32, u64)>,
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
            opened: false,
            occupied: None,
            server,
        })
        .collect();

    let mut events = Vec::new();
    let mut waiting: Vec<usize> = Vec::new();
    let mut customer_index = 0usize;
    let mut finished_count = 0usize;
    let mut now: Option<u64> = None;

    loop {
        let next_time = [
            ordered_customers.get(customer_index).map(|c| c.arrival),
            states.iter().filter(|s| !s.opened).map(|s| s.server.opens).min(),
            states.iter().filter_map(|s| s.occupied.map(|(_, t)| t)).min(),
        ]
        .into_iter()
        .flatten()
        .min();

        let Some(time) = next_time else {
            break;
        };
        if now.is_some_and(|previous| time < previous) {
            return Err(Error::TimeOverflow {
                customer: 0,
                server: 0,
                time,
            });
        }
        now = Some(time);

        let mut finishes: Vec<(u32, usize, u32)> = states
            .iter()
            .enumerate()
            .filter_map(|(i, state)| {
                state.occupied.and_then(|(customer, finish)| {
                    (finish == time).then_some((state.server.id, i, customer))
                })
            })
            .collect();
        finishes.sort_by_key(|entry| entry.0);
        for (_, i, customer) in finishes {
            let server_id = states[i].server.id;
            events.push(Event::Finish {
                time,
                customer,
                server: server_id,
            });
            states[i].occupied = None;
            states[i].idle_since = time;
            finished_count += 1;
        }

        let mut opens: Vec<(u32, usize)> = states
            .iter()
            .enumerate()
            .filter_map(|(i, state)| {
                (!state.opened && state.server.opens == time)
                    .then_some((state.server.id, i))
            })
            .collect();
        opens.sort_by(|a, b| b.0.cmp(&a.0));
        for (_, i) in opens {
            states[i].opened = true;
            events.push(Event::Open {
                time,
                server: states[i].server.id,
            });
        }

        let cohort_start = customer_index;
        while customer_index < ordered_customers.len()
            && ordered_customers[customer_index].arrival == time
        {
            customer_index += 1;
        }
        for customer in &ordered_customers[cohort_start..customer_index] {
            events.push(Event::Arrive {
                time,
                customer: customer.id,
            });
            waiting.push(ordered_customers
                .iter()
                .position(|candidate| candidate.id == customer.id)
                .expect("customer came from ordered list"));
        }

        while !waiting.is_empty() {
            let selected = states
                .iter()
                .enumerate()
                .filter(|(_, state)| state.opened && state.occupied.is_none())
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

            let customer = &ordered_customers[waiting.remove(0)];
            let server = &states[server_index].server;
            let finish = customer
                .service
                .checked_add(time)
                .and_then(|value| value.checked_add(server.overhead))
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
                finished_count += 1;
            } else {
                states[server_index].occupied = Some((customer.id, finish));
            }
        }

        if customer_index == ordered_customers.len()
            && finished_count == ordered_customers.len()
            && states.iter().all(|state| state.opened)
        {
            break;
        }
    }

    Ok(events)
}