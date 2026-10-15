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
    busy_until: Option<u64>,
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

    let mut server_ids = Vec::with_capacity(servers.len());
    for (index, server) in servers.iter().enumerate() {
        if server.id == 0 {
            return Err(Error::ZeroServerId { index });
        }
        if server_ids.contains(&server.id) {
            return Err(Error::DuplicateServerId {
                index,
                id: server.id,
            });
        }
        server_ids.push(server.id);
    }

    let mut customer_ids = Vec::with_capacity(customers.len());
    for (index, customer) in customers.iter().enumerate() {
        if customer.id == 0 {
            return Err(Error::ZeroCustomerId { index });
        }
        if customer_ids.contains(&customer.id) {
            return Err(Error::DuplicateCustomerId {
                index,
                id: customer.id,
            });
        }
        customer_ids.push(customer.id);
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
            busy_until: None,
        })
        .collect();

    let mut events = Vec::new();
    let mut waiting: Vec<Customer> = Vec::new();
    let mut next_customer = 0usize;
    let mut finished_customers = 0usize;

    loop {
        let next_time = states
            .iter()
            .filter_map(|state| {
                if let Some(time) = state.busy_until {
                    Some(time)
                } else if !state.opened {
                    Some(state.server.opens)
                } else {
                    None
                }
            })
            .chain(
                ordered_customers
                    .get(next_customer)
                    .map(|customer| customer.arrival),
            )
            .min();

        let Some(time) = next_time else {
            break;
        };

        let mut completing: Vec<usize> = states
            .iter()
            .enumerate()
            .filter_map(|(index, state)| (state.busy_until == Some(time)).then_some(index))
            .collect();
        completing.sort_by_key(|&index| states[index].server.id);
        for index in completing {
            states[index].busy_until = None;
            states[index].idle_since = time;
            // The customer is identified from the scheduled finish record below.
            // Its identity is stored separately while the server is occupied.
        }

        // Finish records require the customer associated with each occupied server.
        // Track finish ownership by reconstructing it from starts in the trace.
        let mut finish_records: Vec<(usize, u32)> = Vec::new();
        for (index, state) in states.iter().enumerate() {
            if state.opened && state.busy_until.is_none() && state.idle_since == time {
                if let Some(customer) = events.iter().rev().find_map(|event| match event {
                    Event::Start {
                        time: start_time,
                        customer,
                        server,
                        finish,
                    } if *server == state.server.id && *finish == time && *start_time <= time => {
                        Some(*customer)
                    }
                    _ => None,
                }) {
                    finish_records.push((index, customer));
                }
            }
        }
        finish_records.sort_by_key(|(index, _)| states[*index].server.id);
        for (index, customer) in finish_records {
            events.push(Event::Finish {
                time,
                customer,
                server: states[index].server.id,
            });
            finished_customers += 1;
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
            states[index].idle_since = time;
            events.push(Event::Open {
                time,
                server: states[index].server.id,
            });
        }

        let cohort_start = next_customer;
        while next_customer < ordered_customers.len()
            && ordered_customers[next_customer].arrival == time
        {
            next_customer += 1;
        }
        for customer in &ordered_customers[cohort_start..next_customer] {
            waiting.push(customer.clone());
            events.push(Event::Arrive {
                time,
                customer: customer.id,
            });
        }

        while !waiting.is_empty() {
            let Some(server_index) = states
                .iter()
                .enumerate()
                .filter(|(_, state)| state.opened && state.busy_until.is_none())
                .min_by(|(_, a), (_, b)| {
                    a.idle_since
                        .cmp(&b.idle_since)
                        .then_with(|| a.server.overhead.cmp(&b.server.overhead))
                        .then_with(|| b.server.id.cmp(&a.server.id))
                })
                .map(|(index, _)| index)
            else {
                break;
            };

            let customer = waiting.remove(0);
            let server = &states[server_index].server;
            let first_sum = time.checked_add(customer.service).ok_or(Error::TimeOverflow {
                customer: customer.id,
                server: server.id,
                time,
            })?;
            let finish = first_sum
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
                finished_customers += 1;
                states[server_index].idle_since = time;
                states[server_index].busy_until = None;
            } else {
                states[server_index].busy_until = Some(finish);
            }
        }

        if next_customer == ordered_customers.len()
            && finished_customers == customers.len()
            && states.iter().all(|state| state.opened)
        {
            break;
        }
    }

    Ok(events)
}

fn main() {}