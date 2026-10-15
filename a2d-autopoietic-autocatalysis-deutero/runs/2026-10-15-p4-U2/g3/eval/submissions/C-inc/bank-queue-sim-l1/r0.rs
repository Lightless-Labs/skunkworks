#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Customer {
    pub id: u32,
    pub arrival: u64,
    pub service: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Kind {
    Arrive,
    Start { finish: u64, wake: bool },
    Finish,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Event {
    pub time: u64,
    pub customer: u32,
    pub kind: Kind,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Error {
    ZeroId { index: usize },
    DuplicateId { index: usize, id: u32 },
    TimeOverflow { customer: u32, time: u64 },
}

pub fn simulate(customers: &[Customer], wake_cost: u64) -> Result<Vec<Event>, Error> {
    for (index, customer) in customers.iter().enumerate() {
        if customer.id == 0 {
            return Err(Error::ZeroId { index });
        }
        if customers[..index].iter().any(|prior| prior.id == customer.id) {
            return Err(Error::DuplicateId {
                index,
                id: customer.id,
            });
        }
    }

    if customers.is_empty() {
        return Ok(Vec::new());
    }

    let mut arrivals: Vec<&Customer> = customers.iter().collect();
    arrivals.sort_by(|a, b| {
        a.arrival
            .cmp(&b.arrival)
            .then_with(|| b.id.cmp(&a.id))
    });

    let mut events = Vec::with_capacity(customers.len() * 3);
    let mut waiting: Vec<usize> = Vec::new();
    let mut waiting_head = 0usize;
    let mut arrival_index = 0usize;
    let mut completion: Option<(u64, usize)> = None;
    let mut last_finish: Option<u64> = None;
    let mut first_dispatch = true;

    while arrival_index < arrivals.len() || waiting_head < waiting.len() || completion.is_some() {
        let next_arrival = arrivals.get(arrival_index).map(|c| c.arrival);
        let next_completion = completion.map(|(time, _)| time);

        let time = match (next_arrival, next_completion) {
            (Some(a), Some(f)) => a.min(f),
            (Some(a), None) => a,
            (None, Some(f)) => f,
            (None, None) => break,
        };

        if let Some((finish_time, customer_index)) = completion {
            if finish_time == time {
                events.push(Event {
                    time,
                    customer: customers[customer_index].id,
                    kind: Kind::Finish,
                });
                completion = None;
                last_finish = Some(time);
            }
        }

        while arrival_index < arrivals.len() && arrivals[arrival_index].arrival == time {
            let customer = arrivals[arrival_index];
            let customer_index = customers
                .iter()
                .position(|candidate| std::ptr::eq(candidate, customer))
                .unwrap();
            events.push(Event {
                time,
                customer: customer.id,
                kind: Kind::Arrive,
            });
            waiting.push(customer_index);
            arrival_index += 1;
        }

        if completion.is_none() && waiting_head < waiting.len() {
            let customer_index = waiting[waiting_head];
            waiting_head += 1;
            let customer = &customers[customer_index];
            let wake = first_dispatch || last_finish.is_some_and(|finish| time > finish);

            let finish = time
                .checked_add(customer.service)
                .and_then(|t| t.checked_add(if wake { wake_cost } else { 0 }))
                .ok_or(Error::TimeOverflow {
                    customer: customer.id,
                    time,
                })?;

            events.push(Event {
                time,
                customer: customer.id,
                kind: Kind::Start { finish, wake },
            });
            first_dispatch = false;

            if finish == time {
                events.push(Event {
                    time,
                    customer: customer.id,
                    kind: Kind::Finish,
                });
                last_finish = Some(time);
            } else {
                completion = Some((finish, customer_index));
            }
        }

        if waiting_head == waiting.len() {
            waiting.clear();
            waiting_head = 0;
        }
    }

    Ok(events)
}

fn main() {}