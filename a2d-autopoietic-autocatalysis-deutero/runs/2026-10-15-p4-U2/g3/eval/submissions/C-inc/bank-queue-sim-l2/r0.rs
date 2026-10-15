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

    let mut customer_order: Vec<usize> = (0..customers.len()).collect();
    customer_order.sort_by(|&a, &b| {
        customers[a]
            .arrival
            .cmp(&customers[b].arrival)
            .then_with(|| customers[b].id.cmp(&customers[a].id))
    });

    let mut states: Vec<ServerState> = servers
        .iter()
        .cloned()
        .map(|server| ServerState {
            idle_since: server.opens,
            server,
            opened: false,
            occupied_until: None,
        })
        .collect();

    let mut finish_events: Vec<(u64, u32, u32)> = Vec::new();
    let mut waiting: std::collections::VecDeque<usize> = std::collections::VecDeque::new();
    let mut events = Vec::new();
    let mut next_customer = 0;
    let mut completed = 0;

    loop {
        let next_finish = finish_events.iter().map(|entry| entry.0).min();
        let next_open = states
            .iter()
            .filter(|state| !state.opened)
            .map(|state| state.server.opens)
            .min();
        let next_arrival = customer_order
            .get(next_customer)
            .map(|&index| customers[index].arrival);

        let time = [next_finish, next_open, next_arrival]
            .into_iter()
            .flatten()
            .min();

        let Some(time) = time else {
            break;
        };

        let mut finishing = Vec::new();
        let mut remaining_finishes = Vec::new();
        for entry in finish_events.drain(..) {
            if entry.0 == time {
                finishing.push(entry);
            } else {
                remaining_finishes.push(entry);
            }
        }
        finish_events = remaining_finishes;
        finishing.sort_by_key(|entry| entry.1);
        for (_, server_id, customer_id) in finishing {
            if let Some(state) = states.iter_mut().find(|s| s.server.id == server_id) {
                state.occupied_until = None;
                state.idle_since = time;
            }
            events.push(Event::Finish {
                time,
                customer: customer_id,
                server: server_id,
            });
            completed += 1;
        }

        let mut opening_indices: Vec<usize> = states
            .iter()
            .enumerate()
            .filter_map(|(i, state)| {
                (!state.opened && state.server.opens == time).then_some(i)
            })
            .collect();
        opening_indices.sort_by_key(|&i| std::cmp::Reverse(states[i].server.id));
        for index in opening_indices {
            states[index].opened = true;
            events.push(Event::Open {
                time,
                server: states[index].server.id,
            });
        }

        while let Some(&index) = customer_order.get(next_customer) {
            if customers[index].arrival != time {
                break;
            }
            waiting.push_back(index);
            next_customer += 1;
        }

        let mut arriving: Vec<usize> = waiting
            .iter()
            .copied()
            .filter(|&index| customers[index].arrival == time)
            .collect();
        arriving.sort_by_key(|&index| std::cmp::Reverse(customers[index].id));
        for index in arriving {
            events.push(Event::Arrive {
                time,
                customer: customers[index].id,
            });
        }

        while let Some(&customer_index) = waiting.front() {
            let selected = states
                .iter()
                .enumerate()
                .filter(|(_, state)| state.opened && state.occupied_until.is_none())
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

            let customer_id = customers[customer_index].id;
            let service = customers[customer_index].service;
            let server_id = states[server_index].server.id;
            let overhead = states[server_index].server.overhead;

            let finish = time
                .checked_add(service)
                .and_then(|t| t.checked_add(overhead))
                .ok_or(Error::TimeOverflow {
                    customer: customer_id,
                    server: server_id,
                    time,
                })?;

            waiting.pop_front();
            events.push(Event::Start {
                time,
                customer: customer_id,
                server: server_id,
                finish,
            });

            if finish == time {
                states[server_index].idle_since = time;
                events.push(Event::Finish {
                    time,
                    customer: customer_id,
                    server: server_id,
                });
                completed += 1;
            } else {
                states[server_index].occupied_until = Some(finish);
                finish_events.push((finish, server_id, customer_id));
            }
        }

        if next_customer == customers.len()
            && completed == customers.len()
            && states.iter().all(|state| state.opened)
        {
            break;
        }
    }

    Ok(events)
}

fn main() {}