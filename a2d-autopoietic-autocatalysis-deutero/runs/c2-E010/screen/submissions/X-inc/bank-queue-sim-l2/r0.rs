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
    idle_since: u64,
    opened: bool,
    occupied_until: Option<u64>,
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

    let mut customer_order: Vec<usize> = (0..customers.len()).collect();
    customer_order.sort_by(|&a, &b| {
        customers[a]
            .arrival
            .cmp(&customers[b].arrival)
            .then_with(|| customers[b].id.cmp(&customers[a].id))
    });

    let mut states: Vec<ServerState> = servers
        .iter()
        .map(|server| ServerState {
            id: server.id,
            opens: server.opens,
            overhead: server.overhead,
            idle_since: server.opens,
            opened: false,
            occupied_until: None,
        })
        .collect();

    let mut events = Vec::new();
    let mut waiting: Vec<usize> = Vec::new();
    let mut customer_pos = 0;
    let mut finished_count = 0;
    let mut time = 0;

    while finished_count < customers.len() || states.iter().any(|s| !s.opened) {
        // Finish all jobs scheduled for this tick, in ascending server ID order.
        let mut finishing: Vec<usize> = states
            .iter()
            .enumerate()
            .filter_map(|(i, state)| {
                (state.occupied_until == Some(time)).then_some(i)
            })
            .collect();
        finishing.sort_by_key(|&i| states[i].id);
        for i in finishing {
            let state = &mut states[i];
            let customer = match events.last() {
                Some(Event::Start { customer, server, finish, .. })
                    if *server == state.id && *finish == time => *customer,
                _ => {
                    // Find the active customer from the matching scheduled start.
                    events.iter().rev().find_map(|event| match event {
                        Event::Start {
                            customer,
                            server,
                            finish,
                            ..
                        } if *server == state.id && *finish == time => Some(*customer),
                        _ => None,
                    }).expect("occupied server has a matching start")
                }
            };
            events.push(Event::Finish {
                time,
                customer,
                server: state.id,
            });
            state.occupied_until = None;
            state.idle_since = time;
            finished_count += 1;
        }

        // Open servers, in descending server ID order.
        let mut opening: Vec<usize> = states
            .iter()
            .enumerate()
            .filter_map(|(i, state)| {
                (!state.opened && state.opens == time).then_some(i)
            })
            .collect();
        opening.sort_by(|&a, &b| states[b].id.cmp(&states[a].id));
        for i in opening {
            states[i].opened = true;
            events.push(Event::Open {
                time,
                server: states[i].id,
            });
        }

        // Emit the complete arrival cohort, already ordered by descending ID.
        while customer_pos < customer_order.len()
            && customers[customer_order[customer_pos]].arrival == time
        {
            let index = customer_order[customer_pos];
            events.push(Event::Arrive {
                time,
                customer: customers[index].id,
            });
            waiting.push(index);
            customer_pos += 1;
        }

        // Dispatch FIFO customers using the specified server ranking.
        while !waiting.is_empty() {
            let selected = states
                .iter()
                .enumerate()
                .filter(|(_, state)| state.opened && state.occupied_until.is_none())
                .min_by(|(_, a), (_, b)| {
                    a.idle_since
                        .cmp(&b.idle_since)
                        .then_with(|| a.overhead.cmp(&b.overhead))
                        .then_with(|| b.id.cmp(&a.id))
                })
                .map(|(i, _)| i);

            let Some(server_index) = selected else {
                break;
            };

            let customer_index = waiting.remove(0);
            let customer = &customers[customer_index];
            let server_id = states[server_index].id;
            let finish = time
                .checked_add(customer.service)
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
                finished_count += 1;
            } else {
                states[server_index].occupied_until = Some(finish);
            }
        }

        if finished_count >= customers.len() && states.iter().all(|s| s.opened) {
            break;
        }

        let next_arrival = customer_order
            .get(customer_pos)
            .map(|&i| customers[i].arrival);
        let next_open = states
            .iter()
            .filter(|state| !state.opened)
            .map(|state| state.opens)
            .min();
        let next_finish = states
            .iter()
            .filter_map(|state| state.occupied_until)
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