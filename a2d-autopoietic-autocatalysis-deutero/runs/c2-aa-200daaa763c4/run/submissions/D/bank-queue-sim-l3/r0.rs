#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Customer {
    pub id: u32,
    pub arrival: u64,
    pub service: u64,
    pub priority: u8,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Kind {
    Arrive,
    Start {
        remaining: u64,
        work_at: u64,
        finish: u64,
    },
    Resume {
        remaining: u64,
        work_at: u64,
        finish: u64,
    },
    Preempt {
        remaining: u64,
        by: u32,
    },
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
    BadPriority { index: usize, value: u8 },
    TimeOverflow { customer: u32, time: u64 },
}

#[derive(Clone)]
struct Job {
    customer: Customer,
    started: bool,
    remaining: u64,
    pause_time: Option<u64>,
}

fn waiter_precedes(a: &Job, b: &Job) -> std::cmp::Ordering {
    use std::cmp::Ordering;

    a.customer
        .priority
        .cmp(&b.customer.priority)
        .then_with(|| match (a.pause_time, b.pause_time) {
            (Some(at), Some(bt)) => at
                .cmp(&bt)
                .then_with(|| a.customer.id.cmp(&b.customer.id)),
            (Some(_), None) => Ordering::Less,
            (None, Some(_)) => Ordering::Greater,
            (None, None) => b
                .customer
                .arrival
                .cmp(&a.customer.arrival)
                .then_with(|| b.customer.id.cmp(&a.customer.id)),
        })
}

fn best_waiter(waiting: &[usize], jobs: &[Job]) -> Option<usize> {
    waiting
        .iter()
        .copied()
        .min_by(|&a, &b| waiter_precedes(&jobs[a], &jobs[b]))
}

fn remove_waiter(waiting: &mut Vec<usize>, idx: usize) {
    if let Some(pos) = waiting.iter().position(|&candidate| candidate == idx) {
        waiting.swap_remove(pos);
    }
}

pub fn simulate(customers: &[Customer], switch_cost: u64) -> Result<Vec<Event>, Error> {
    use std::collections::HashSet;

    let mut seen = HashSet::new();
    for (index, customer) in customers.iter().enumerate() {
        if customer.id == 0 {
            return Err(Error::ZeroId { index });
        }
        if !seen.insert(customer.id) {
            return Err(Error::DuplicateId {
                index,
                id: customer.id,
            });
        }
        if customer.priority > 3 {
            return Err(Error::BadPriority {
                index,
                value: customer.priority,
            });
        }
    }

    if customers.is_empty() {
        return Ok(Vec::new());
    }

    let mut jobs: Vec<Job> = customers
        .iter()
        .cloned()
        .map(|customer| Job {
            remaining: customer.service,
            customer,
            started: false,
            pause_time: None,
        })
        .collect();

    let mut arrivals: Vec<usize> = (0..jobs.len()).collect();
    arrivals.sort_by(|&a, &b| {
        let a = &jobs[a].customer;
        let b = &jobs[b].customer;
        a.arrival
            .cmp(&b.arrival)
            .then_with(|| a.priority.cmp(&b.priority))
            .then_with(|| b.id.cmp(&a.id))
    });

    let mut events = Vec::new();
    let mut waiting: Vec<usize> = Vec::new();
    let mut arrival_cursor = 0usize;
    let mut active: Option<(usize, u64, u64)> = None;
    let mut last_dispatched: Option<u32> = None;
    let mut completed = 0usize;
    let mut now = 0u64;

    while completed < jobs.len() {
        let next_arrival = arrivals
            .get(arrival_cursor)
            .map(|&idx| jobs[idx].customer.arrival);

        let next_tick = match (next_arrival, active) {
            (Some(arrival), Some((_, work_at, finish))) => [arrival, work_at, finish]
                .into_iter()
                .filter(|&time| time > now)
                .min()
                .or_else(|| {
                    [Some(arrival), Some(work_at), Some(finish)]
                        .into_iter()
                        .flatten()
                        .filter(|&time| time == now)
                        .min()
                }),
            (Some(arrival), None) => Some(arrival),
            (None, Some((_, work_at, finish))) => [work_at, finish]
                .into_iter()
                .filter(|&time| time > now)
                .min()
                .or_else(|| [work_at, finish].into_iter().find(|&time| time == now)),
            (None, None) => None,
        };

        let Some(tick) = next_tick else {
            break;
        };
        now = tick;

        if let Some((idx, _, finish)) = active {
            if finish == now {
                events.push(Event {
                    time: now,
                    customer: jobs[idx].customer.id,
                    kind: Kind::Finish,
                });
                active = None;
                completed += 1;
            }
        }

        while arrival_cursor < arrivals.len()
            && jobs[arrivals[arrival_cursor]].customer.arrival == now
        {
            let idx = arrivals[arrival_cursor];
            events.push(Event {
                time: now,
                customer: jobs[idx].customer.id,
                kind: Kind::Arrive,
            });
            waiting.push(idx);
            arrival_cursor += 1;
        }

        if let Some((idx, work_at, finish)) = active {
            if now >= work_at && now < finish {
                if let Some(best_idx) = best_waiter(&waiting, &jobs) {
                    if jobs[best_idx].customer.priority < jobs[idx].customer.priority {
                        let by = jobs[best_idx].customer.id;
                        let remaining = finish - now;
                        events.push(Event {
                            time: now,
                            customer: jobs[idx].customer.id,
                            kind: Kind::Preempt { remaining, by },
                        });
                        active = None;
                        jobs[idx].remaining = remaining;
                        jobs[idx].pause_time = Some(now);
                        waiting.push(idx);
                    }
                }
            }
        }

        if active.is_none() {
            while let Some(idx) = best_waiter(&waiting, &jobs) {
                remove_waiter(&mut waiting, idx);
                let id = jobs[idx].customer.id;
                let cost = if last_dispatched.is_some_and(|previous| previous != id) {
                    switch_cost
                } else {
                    0
                };

                let work_at = now.checked_add(cost).ok_or(Error::TimeOverflow {
                    customer: id,
                    time: now,
                })?;
                let finish = work_at
                    .checked_add(jobs[idx].remaining)
                    .ok_or(Error::TimeOverflow {
                        customer: id,
                        time: now,
                    })?;

                let kind = if jobs[idx].started {
                    Kind::Resume {
                        remaining: jobs[idx].remaining,
                        work_at,
                        finish,
                    }
                } else {
                    Kind::Start {
                        remaining: jobs[idx].remaining,
                        work_at,
                        finish,
                    }
                };

                jobs[idx].started = true;
                jobs[idx].pause_time = None;
                last_dispatched = Some(id);
                events.push(Event {
                    time: now,
                    customer: id,
                    kind,
                });

                if finish == now {
                    events.push(Event {
                        time: now,
                        customer: id,
                        kind: Kind::Finish,
                    });
                    completed += 1;
                    continue;
                }

                active = Some((idx, work_at, finish));
                break;
            }
        }
    }

    Ok(events)
}