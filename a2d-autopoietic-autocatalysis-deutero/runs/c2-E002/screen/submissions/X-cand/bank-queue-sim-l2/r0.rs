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
    id: u32,
    opens: u64,
    overhead: u64,
    is_open: bool,
    idle_since: u64,
    occupied: Option<(u64, u32)>,
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

    let mut server_states: Vec<ServerState> = servers
        .iter()
        .map(|server| ServerState {
            id: server.id,
            opens: server.opens,
            overhead: server.overhead,
            is_open: false,
            idle_since: server.opens,
            occupied: None,
        })
        .collect();

    let mut events = Vec::new();
    let mut waiting: Vec<&Customer> = Vec::new();
    let mut next_arrival = 0;
    let mut opened_count = 0;
    let mut finished_count = 0;
    let mut time = 0;

    loop {
        // Complete scheduled jobs first, ordered by server ID ascending.
        let mut finishing: Vec<usize> = server_states
            .iter()
            .enumerate()
            .filter_map(|(index, state)| {
                state
                    .occupied
                    .filter(|(finish, _)| *finish == time)
                    .map(|_| index)
            })
            .collect();
        finishing.sort_by_key(|&index| server_states[index].id);

        for index in finishing {
            let state = &mut server_states[index];
            let (_, customer) = state.occupied.take().unwrap();
            events.push(Event::Finish {
                time,
                customer,
                server: state.id,
            });
            state.idle_since = time;
            finished_count += 1;
        }

        // Open servers in descending ID order.
        let mut opening: Vec<usize> = server_states
            .iter()
            .enumerate()
            .filter_map(|(index, state)| {
                (!state.is_open && state.opens == time).then_some(index)
            })
            .collect();
        opening.sort_by_key(|&index| std::cmp::Reverse(server_states[index].id));

        for index in opening {
            let state = &mut server_states[index];
            state.is_open = true;
            events.push(Event::Open {
                time,
                server: state.id,
            });
            opened_count += 1;
        }

        // Emit the complete arrival cohort in descending customer ID order.
        let cohort_start = next_arrival;
        while next_arrival < arrivals.len() && arrivals[next_arrival].arrival == time {
            next_arrival += 1;
        }
        for customer in &arrivals[cohort_start..next_arrival] {
            events.push(Event::Arrive {
                time,
                customer: customer.id,
            });
            waiting.push(customer);
        }

        // Dispatch in FIFO order, recalculating eligible-server ranking each time.
        while !waiting.is_empty() {
            let selected = server_states
                .iter()
                .enumerate()
                .filter(|(_, state)| state.is_open && state.occupied.is_none())
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
            let server_id = server_states[server_index].id;
            let finish = customer
                .service
                .checked_add(time)
                .and_then(|t| t.checked_add(server_states[server_index].overhead))
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
                server_states[server_index].idle_since = time;
                finished_count += 1;
            } else {
                server_states[server_index].occupied = Some((finish, customer.id));
            }
        }

        if opened_count == servers.len()
            && finished_count == customers.len()
            && waiting.is_empty()
            && next_arrival == arrivals.len()
        {
            break;
        }

        let mut next_time: Option<u64> = None;
        let mut consider = |candidate: u64| {
            if candidate > time {
                next_time = Some(next_time.map_or(candidate, |current| current.min(candidate)));
            }
        };

        if next_arrival < arrivals.len() {
            consider(arrivals[next_arrival].arrival);
        }
        for state in &server_states {
            if !state.is_open {
                consider(state.opens);
            }
            if let Some((finish, _)) = state.occupied {
                consider(finish);
            }
        }

        time = next_time.expect("simulation has a future event until it is complete");
    }

    Ok(events)
}