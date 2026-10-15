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
struct RuntimeServer {
    id: u32,
    opens: u64,
    overhead: u64,
    is_open: bool,
    busy: bool,
    idle_since: u64,
    finish_at: Option<u64>,
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
    arrivals.sort_by(|a, b| a.arrival.cmp(&b.arrival).then_with(|| b.id.cmp(&a.id)));

    let mut runtime_servers: Vec<RuntimeServer> = servers
        .iter()
        .map(|server| RuntimeServer {
            id: server.id,
            opens: server.opens,
            overhead: server.overhead,
            is_open: false,
            busy: false,
            idle_since: server.opens,
            finish_at: None,
        })
        .collect();

    let mut waiting: Vec<&Customer> = Vec::new();
    let mut next_arrival = 0usize;
    let mut events = Vec::new();
    let mut now = 0u64;

    loop {
        // Finish events are emitted in ascending server ID order.
        let mut finishing: Vec<usize> = runtime_servers
            .iter()
            .enumerate()
            .filter_map(|(i, server)| {
                (server.busy && server.finish_at == Some(now)).then_some(i)
            })
            .collect();
        finishing.sort_by_key(|&i| runtime_servers[i].id);

        for i in finishing {
            let server = &mut runtime_servers[i];
            let customer_id = server
                .finish_at
                .and_then(|_| {
                    // The customer is tracked separately by the active finish record below.
                    None::<u32>
                });
            let _ = customer_id;
        }

        // Active customer IDs for occupied servers are reconstructed from finish records.
        // Store them in a parallel pass through the event history.
        let mut finish_customer_ids = vec![None; runtime_servers.len()];
        for event in events.iter().rev() {
            if let Event::Start {
                customer,
                server,
                finish,
                ..
            } = event
            {
                if *finish >= now {
                    if let Some(index) = runtime_servers.iter().position(|s| s.id == *server) {
                        if runtime_servers[index].busy
                            && runtime_servers[index].finish_at == Some(*finish)
                            && finish_customer_ids[index].is_none()
                        {
                            finish_customer_ids[index] = Some(*customer);
                        }
                    }
                }
            }
        }

        for i in runtime_servers
            .iter()
            .enumerate()
            .filter_map(|(i, server)| {
                (server.busy && server.finish_at == Some(now)).then_some(i)
            })
            .collect::<Vec<_>>()
            .into_iter()
            .sorted_by_key(|&i| runtime_servers[i].id)
        {
            let server = &mut runtime_servers[i];
            let customer = finish_customer_ids[i]
                .expect("busy server must have a matching start event");
            events.push(Event::Finish {
                time: now,
                customer,
                server: server.id,
            });
            server.busy = false;
            server.finish_at = None;
            server.idle_since = now;
        }

        // Opens in descending server ID order.
        let mut opening: Vec<usize> = runtime_servers
            .iter()
            .enumerate()
            .filter_map(|(i, server)| {
                (!server.is_open && server.opens == now).then_some(i)
            })
            .collect();
        opening.sort_by_key(|&i| std::cmp::Reverse(runtime_servers[i].id));
        for i in opening {
            let server = &mut runtime_servers[i];
            server.is_open = true;
            server.idle_since = now;
            events.push(Event::Open {
                time: now,
                server: server.id,
            });
        }

        // Arrivals in descending customer ID order within this tick.
        while next_arrival < arrivals.len() && arrivals[next_arrival].arrival == now {
            let arrival_time = arrivals[next_arrival].arrival;
            let mut end = next_arrival + 1;
            while end < arrivals.len() && arrivals[end].arrival == arrival_time {
                end += 1;
            }
            for customer in &arrivals[next_arrival..end] {
                events.push(Event::Arrive {
                    time: now,
                    customer: customer.id,
                });
                waiting.push(customer);
            }
            next_arrival = end;
        }

        // Dispatch FIFO customers to the best currently idle server.
        while !waiting.is_empty() {
            let selected = runtime_servers
                .iter()
                .enumerate()
                .filter(|(_, server)| server.is_open && !server.busy)
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

            let customer = waiting.remove(0);
            let server_id = runtime_servers[server_index].id;
            let finish = customer
                .arrival
                .max(now)
                .checked_add(customer.service)
                .and_then(|t| t.checked_add(runtime_servers[server_index].overhead))
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
                runtime_servers[server_index].idle_since = now;
            } else {
                runtime_servers[server_index].busy = true;
                runtime_servers[server_index].finish_at = Some(finish);
            }
        }

        let all_done = next_arrival == arrivals.len() && waiting.is_empty()
            && runtime_servers.iter().all(|server| !server.busy);
        let all_opened = runtime_servers.iter().all(|server| server.is_open);
        if all_done && all_opened {
            break;
        }

        let next_time = arrivals
            .get(next_arrival)
            .map(|customer| customer.arrival)
            .into_iter()
            .chain(
                runtime_servers
                    .iter()
                    .filter(|server| !server.is_open)
                    .map(|server| server.opens),
            )
            .chain(
                runtime_servers
                    .iter()
                    .filter(|server| server.busy)
                    .filter_map(|server| server.finish_at),
            )
            .filter(|&time| time > now)
            .min();

        match next_time {
            Some(time) => now = time,
            None if all_done => break,
            None => break,
        }
    }

    Ok(events)
}

trait SortByKeyExt<T> {
    fn sorted_by_key<K: Ord, F: FnMut(&T) -> K>(self, f: F) -> Vec<T>;
}

impl<T, I: Iterator<Item = T>> SortByKeyExt<T> for I {
    fn sorted_by_key<K: Ord, F: FnMut(&T) -> K>(self, mut f: F) -> Vec<T> {
        let mut values: Vec<T> = self.collect();
        values.sort_by_key(|value| f(value));
        values
    }
}

fn main() {}