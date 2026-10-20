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
    open: bool,
    busy_until: Option<u64>,
    active_customer: Option<u32>,
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

    let mut seen_servers = Vec::new();
    for (index, server) in servers.iter().enumerate() {
        if server.id == 0 {
            return Err(Error::ZeroServerId { index });
        }
        if seen_servers.contains(&server.id) {
            return Err(Error::DuplicateServerId {
                index,
                id: server.id,
            });
        }
        seen_servers.push(server.id);
    }

    let mut seen_customers = Vec::new();
    for (index, customer) in customers.iter().enumerate() {
        if customer.id == 0 {
            return Err(Error::ZeroCustomerId { index });
        }
        if seen_customers.contains(&customer.id) {
            return Err(Error::DuplicateCustomerId {
                index,
                id: customer.id,
            });
        }
        seen_customers.push(customer.id);
    }

    let mut queue = customers.to_vec();
    queue.sort_by(|a, b| {
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
            open: false,
            busy_until: None,
            active_customer: None,
        })
        .collect();

    let mut events = Vec::new();
    let mut waiting = std::collections::VecDeque::<usize>::new();
    let mut next_arrival = 0usize;
    let mut now = 0u64;

    loop {
        let mut completions: Vec<usize> = states
            .iter()
            .enumerate()
            .filter_map(|(i, state)| {
                if state.busy_until == Some(now) {
                    Some(i)
                } else {
                    None
                }
            })
            .collect();
        completions.sort_by_key(|&i| states[i].server.id);

        for i in completions {
            let customer = states[i].active_customer.take().unwrap();
            events.push(Event::Finish {
                time: now,
                customer,
                server: states[i].server.id,
            });
            states[i].busy_until = None;
            states[i].idle_since = now;
        }

        let mut openings: Vec<usize> = states
            .iter()
            .enumerate()
            .filter_map(|(i, state)| {
                if !state.open && state.server.opens == now {
                    Some(i)
                } else {
                    None
                }
            })
            .collect();
        openings.sort_by_key(|&i| std::cmp::Reverse(states[i].server.id));

        for i in openings {
            states[i].open = true;
            events.push(Event::Open {
                time: now,
                server: states[i].server.id,
            });
        }

        let cohort_begin = next_arrival;
        while next_arrival < queue.len() && queue[next_arrival].arrival == now {
            next_arrival += 1;
        }
        for index in cohort_begin..next_arrival {
            events.push(Event::Arrive {
                time: now,
                customer: queue[index].id,
            });
            waiting.push_back(index);
        }

        loop {
            let Some(&customer_index) = waiting.front() else {
                break;
            };
            let server_index = states
                .iter()
                .enumerate()
                .filter(|(_, state)| state.open && state.busy_until.is_none())
                .min_by(|(_, a), (_, b)| {
                    a.idle_since
                        .cmp(&b.idle_since)
                        .then_with(|| a.server.overhead.cmp(&b.server.overhead))
                        .then_with(|| b.server.id.cmp(&a.server.id))
                })
                .map(|(i, _)| i);

            let Some(server_index) = server_index else {
                break;
            };

            waiting.pop_front();
            let customer = &queue[customer_index];
            let server = &states[server_index].server;
            let finish = now
                .checked_add(customer.service)
                .and_then(|time| time.checked_add(server.overhead))
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
            } else {
                states[server_index].busy_until = Some(finish);
                states[server_index].active_customer = Some(customer.id);
            }
        }

        if next_arrival == queue.len()
            && waiting.is_empty()
            && states.iter().all(|state| state.busy_until.is_none())
            && states.iter().all(|state| state.open)
        {
            break;
        }

        let mut next = None;
        let mut add_candidate = |candidate: u64| {
            if candidate > now && next.map_or(true, |current| candidate < current) {
                next = Some(candidate);
            }
        };

        if next_arrival < queue.len() {
            add_candidate(queue[next_arrival].arrival);
        }
        for state in &states {
            if !state.open {
                add_candidate(state.server.opens);
            }
            if let Some(time) = state.busy_until {
                add_candidate(time);
            }
        }

        if let Some(time) = next {
            now = time;
        } else {
            break;
        }
    }

    Ok(events)
}
