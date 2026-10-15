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
    busy: bool,
    idle_since: u64,
    finish_at: u64,
    customer: u32,
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
            opened: false,
            busy: false,
            idle_since: server.opens,
            finish_at: 0,
            customer: 0,
        })
        .collect();

    let mut events = Vec::new();
    let mut waiting: Vec<usize> = Vec::new();
    let mut next_arrival = 0;
    let mut completed_customers = 0;
    let mut opened_servers = 0;
    let mut now = 0u64;

    while completed_customers < customers.len() || opened_servers < states.len() {
        // Finish events, ordered by ascending server ID.
        let mut finishing: Vec<usize> = states
            .iter()
            .enumerate()
            .filter_map(|(i, state)| {
                (state.busy && state.finish_at == now).then_some(i)
            })
            .collect();
        finishing.sort_by_key(|&i| states[i].id);

        for i in finishing {
            let customer = states[i].customer;
            let server = states[i].id;
            events.push(Event::Finish {
                time: now,
                customer,
                server,
            });
            states[i].busy = false;
            states[i].idle_since = now;
            completed_customers += 1;
        }

        // Open events, ordered by descending server ID.
        let mut opening: Vec<usize> = states
            .iter()
            .enumerate()
            .filter_map(|(i, state)| {
                (!state.opened && state.opens == now).then_some(i)
            })
            .collect();
        opening.sort_by(|&a, &b| states[b].id.cmp(&states[a].id));

        for i in opening {
            states[i].opened = true;
            opened_servers += 1;
            events.push(Event::Open {
                time: now,
                server: states[i].id,
            });
        }

        // Arrive events, ordered by descending customer ID within the tick.
        while next_arrival < customer_order.len()
            && customers[customer_order[next_arrival]].arrival == now
        {
            let customer_index = customer_order[next_arrival];
            events.push(Event::Arrive {
                time: now,
                customer: customers[customer_index].id,
            });
            waiting.push(customer_index);
            next_arrival += 1;
        }

        // Dispatch in FIFO order, re-ranking idle servers after each start.
        while !waiting.is_empty() {
            let selected = states
                .iter()
                .enumerate()
                .filter(|(_, state)| state.opened && !state.busy)
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

            let customer_index = waiting.remove(0);
            let customer = &customers[customer_index];
            let server_id = states[server_index].id;
            let finish = now
                .checked_add(customer.service)
                .and_then(|time| time.checked_add(states[server_index].overhead))
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
                states[server_index].busy = false;
                states[server_index].idle_since = now;
                completed_customers += 1;
            } else {
                states[server_index].busy = true;
                states[server_index].finish_at = finish;
                states[server_index].customer = customer.id;
            }
        }

        if completed_customers == customers.len() && opened_servers == states.len() {
            break;
        }

        let next = customer_order
            .get(next_arrival)
            .map(|&i| customers[i].arrival)
            .into_iter()
            .chain(
                states
                    .iter()
                    .filter(|state| !state.opened)
                    .map(|state| state.opens),
            )
            .chain(
                states
                    .iter()
                    .filter(|state| state.busy)
                    .map(|state| state.finish_at),
            )
            .filter(|&time| time > now)
            .min();

        if let Some(time) = next {
            now = time;
        } else {
            break;
        }
    }

    Ok(events)
}