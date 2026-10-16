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
    pause_time: Option<u64>,
    remaining: u64,
}

#[derive(Clone)]
struct Active {
    index: usize,
    work_at: u64,
    finish: u64,
    remaining: u64,
}

fn waiter_better(a: &Job, b: &Job) -> bool {
    if a.customer.priority != b.customer.priority {
        return a.customer.priority < b.customer.priority;
    }
    match (a.pause_time, b.pause_time) {
        (Some(pa), Some(pb)) => (pa, a.customer.id) < (pb, b.customer.id),
        (Some(_), None) => true,
        (None, Some(_)) => false,
        (None, None) => {
            (a.customer.arrival, std::cmp::Reverse(a.customer.id))
                < (b.customer.arrival, std::cmp::Reverse(b.customer.id))
        }
    }
}

fn best_waiter(waiting: &[usize], jobs: &[Job]) -> Option<usize> {
    waiting.iter().copied().min_by(|&a, &b| {
        if waiter_better(&jobs[a], &jobs[b]) {
            std::cmp::Ordering::Less
        } else if waiter_better(&jobs[b], &jobs[a]) {
            std::cmp::Ordering::Greater
        } else {
            std::cmp::Ordering::Equal
        }
    })
}

pub fn simulate(customers: &[Customer], switch_cost: u64) -> Result<Vec<Event>, Error> {
    for (index, customer) in customers.iter().enumerate() {
        if customer.id == 0 {
            return Err(Error::ZeroId { index });
        }
        if customers[..index].iter().any(|c| c.id == customer.id) {
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
        let ca = &jobs[a].customer;
        let cb = &jobs[b].customer;
        ca.arrival
            .cmp(&cb.arrival)
            .then_with(|| ca.priority.cmp(&cb.priority))
            .then_with(|| cb.id.cmp(&ca.id))
    });

    let mut next_arrival = 0usize;
    let mut waiting: Vec<usize> = Vec::new();
    let mut active: Option<Active> = None;
    let mut last_dispatched: Option<u32> = None;
    let mut now = 0u64;
    let mut events = Vec::new();

    loop {
        if let Some(current) = active.as_ref() {
            if current.finish == now {
                let idx = current.index;
                events.push(Event {
                    time: now,
                    customer: jobs[idx].customer.id,
                    kind: Kind::Finish,
                });
                active = None;
            }
        }

        while next_arrival < arrivals.len()
            && jobs[arrivals[next_arrival]].customer.arrival == now
        {
            let idx = arrivals[next_arrival];
            events.push(Event {
                time: now,
                customer: jobs[idx].customer.id,
                kind: Kind::Arrive,
            });
            waiting.push(idx);
            next_arrival += 1;
        }

        if let Some(current) = active.as_ref() {
            if now >= current.work_at {
                if let Some(waiter) = best_waiter(&waiting, &jobs) {
                    if jobs[waiter].customer.priority < jobs[current.index].customer.priority {
                        let current = active.take().unwrap();
                        let id = jobs[current.index].customer.id;
                        let by = jobs[waiter].customer.id;
                        let remaining = current.finish - now;
                        events.push(Event {
                            time: now,
                            customer: id,
                            kind: Kind::Preempt { remaining, by },
                        });
                        jobs[current.index].remaining = remaining;
                        jobs[current.index].pause_time = Some(now);
                        waiting.push(current.index);
                    }
                }
            }
        }

        if active.is_none() {
            while let Some(idx) = best_waiter(&waiting, &jobs) {
                waiting.retain(|&candidate| candidate != idx);
                let id = jobs[idx].customer.id;
                let remaining = jobs[idx].remaining;
                let switching = last_dispatched.map_or(false, |last| last != id);
                let cost = if switching { switch_cost } else { 0 };
                let work_at = now.checked_add(cost).ok_or(Error::TimeOverflow {
                    customer: id,
                    time: now,
                })?;
                let finish = work_at.checked_add(remaining).ok_or(Error::TimeOverflow {
                    customer: id,
                    time: now,
                })?;

                let kind = if jobs[idx].started {
                    Kind::Resume {
                        remaining,
                        work_at,
                        finish,
                    }
                } else {
                    Kind::Start {
                        remaining,
                        work_at,
                        finish,
                    }
                };
                events.push(Event {
                    time: now,
                    customer: id,
                    kind,
                });
                jobs[idx].started = true;
                jobs[idx].pause_time = None;
                last_dispatched = Some(id);

                if finish == now {
                    events.push(Event {
                        time: now,
                        customer: id,
                        kind: Kind::Finish,
                    });
                    continue;
                }
                active = Some(Active {
                    index: idx,
                    work_at,
                    finish,
                    remaining,
                });
                break;
            }
        }

        if next_arrival == arrivals.len() && active.is_none() && waiting.is_empty() {
            break;
        }

        let mut next = None;
        if next_arrival < arrivals.len() {
            next = Some(jobs[arrivals[next_arrival]].customer.arrival);
        }
        if let Some(current) = active.as_ref() {
            if current.finish > now {
                next = Some(next.map_or(current.finish, |t| t.min(current.finish)));
            }
            if current.work_at > now {
                next = Some(next.map_or(current.work_at, |t| t.min(current.work_at)));
            }
        }

        if let Some(next_time) = next {
            now = next_time;
        } else {
            break;
        }
    }

    Ok(events)
}