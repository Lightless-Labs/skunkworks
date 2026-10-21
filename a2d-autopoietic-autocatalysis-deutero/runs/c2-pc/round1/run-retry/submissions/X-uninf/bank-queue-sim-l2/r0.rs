use std::collections::{HashSet, VecDeque};

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
    is_open: bool,
    busy: bool,
    idle_since: u64,
    finish: Option<(u64, u32)>,
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

    let mut server_ids = HashSet::with_capacity(servers.len());
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

    let mut customer_ids = HashSet::with_capacity(customers.len());
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

    // Keep original indices so arrival handling never has to search the input.
    let mut ordered_customers: Vec<usize> = (0..customers.len()).collect();
    ordered_customers.sort_by(|&a, &b| {
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
            is_open: false,
            busy: false,
            idle_since: server.opens,
            finish: None,
        })
        .collect();

    let mut events = Vec::new();
    let mut waiting: VecDeque<usize> = VecDeque::new();
    let mut customer_index = 0usize;
    let mut time = 0u64;

    loop {
        let mut finishing: Vec<usize> = states
            .iter()
            .enumerate()
            .filter_map(|(i, state)| {
                state
                    .finish
                    .filter(|(finish_time, _)| *finish_time == time)
                    .map(|_| i)
            })
            .collect();
        finishing.sort_by_key(|&i| states[i].id);
        for i in finishing {
            let (finish_time, customer) = states[i].finish.take().unwrap();
            states[i].busy = false;
            states[i].idle_since = finish_time;
            events.push(Event::Finish {
                time,
                customer,
                server: states[i].id,
            });
        }

        let mut opening: Vec<usize> = states
            .iter()
            .enumerate()
            .filter_map(|(i, state)| (!state.is_open && state.opens == time).then_some(i))
            .collect();
        opening.sort_by_key(|&i| std::cmp::Reverse(states[i].id));
        for i in opening {
            states[i].is_open = true;
            states[i].idle_since = time;
            events.push(Event::Open {
                time,
                server: states[i].id,
            });
        }

        let cohort_start = customer_index;
        while customer_index < ordered_customers.len()
            && customers[ordered_customers[customer_index]].arrival == time
        {
            customer_index += 1;
        }
        for &customer_index_in_input in &ordered_customers[cohort_start..customer_index] {
            let customer = &customers[customer_index_in_input];
            events.push(Event::Arrive {
                time,
                customer: customer.id,
            });
            waiting.push_back(customer_index_in_input);
        }

        while let Some(&customer_index_in_input) = waiting.front() {
            let selected_server = states
                .iter()
                .enumerate()
                .filter(|(_, state)| state.is_open && !state.busy)
                .min_by(|(_, a), (_, b)| {
                    a.idle_since
                        .cmp(&b.idle_since)
                        .then_with(|| a.overhead.cmp(&b.overhead))
                        .then_with(|| b.id.cmp(&a.id))
                })
                .map(|(i, _)| i);

            let Some(server_index) = selected_server else {
                break;
            };

            waiting.pop_front();
            let customer = &customers[customer_index_in_input];
            let server_id = states[server_index].id;
            let after_service =
                time.checked_add(customer.service)
                    .ok_or(Error::TimeOverflow {
                        customer: customer.id,
                        server: server_id,
                        time,
                    })?;
            let finish = after_service
                .checked_add(states[server_index].overhead)
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
                states[server_index].busy = false;
                states[server_index].idle_since = time;
                events.push(Event::Finish {
                    time,
                    customer: customer.id,
                    server: server_id,
                });
            } else {
                states[server_index].busy = true;
                states[server_index].finish = Some((finish, customer.id));
            }
        }

        let all_finished = customer_index == ordered_customers.len()
            && waiting.is_empty()
            && states.iter().all(|state| !state.busy);
        let all_opened = states.iter().all(|state| state.is_open);
        if all_finished && all_opened {
            break;
        }

        let next_arrival = ordered_customers
            .get(customer_index)
            .map(|&index| customers[index].arrival);
        let next_open = states
            .iter()
            .filter(|state| !state.is_open)
            .map(|state| state.opens)
            .min();
        let next_finish = states
            .iter()
            .filter_map(|state| state.finish.map(|(finish_time, _)| finish_time))
            .min();

        let next_time = [next_arrival, next_open, next_finish]
            .into_iter()
            .flatten()
            .filter(|&candidate| candidate > time)
            .min();

        match next_time {
            Some(next_time) => time = next_time,
            None => break,
        }
    }

    Ok(events)
}