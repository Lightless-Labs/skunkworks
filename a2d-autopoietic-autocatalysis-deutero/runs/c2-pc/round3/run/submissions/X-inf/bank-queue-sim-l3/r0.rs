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
    remaining: u64,
    started: bool,
    pause_time: Option<u64>,
}

#[derive(Clone)]
struct Running {
    index: usize,
    work_at: u64,
    finish: u64,
}

fn better(a: &Job, b: &Job) -> bool {
    if a.customer.priority != b.customer.priority {
        return a.customer.priority < b.customer.priority;
    }

    match (a.pause_time, b.pause_time) {
        (Some(pa), Some(pb)) => {
            pa < pb || (pa == pb && a.customer.id < b.customer.id)
        }
        (Some(_), None) => true,
        (None, Some(_)) => false,
        (None, None) => {
            a.customer.arrival < b.customer.arrival
                || (a.customer.arrival == b.customer.arrival
                    && a.customer.id > b.customer.id)
        }
    }
}

fn best_waiter(waiting: &[usize], jobs: &[Job]) -> Option<usize> {
    waiting
        .iter()
        .copied()
        .min_by(|&a, &b| {
            if better(&jobs[a], &jobs[b]) {
                std::cmp::Ordering::Less
            } else if better(&jobs[b], &jobs[a]) {
                std::cmp::Ordering::Greater
            } else {
                std::cmp::Ordering::Equal
            }
        })
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

    let mut order: Vec<usize> = (0..customers.len()).collect();
    order.sort_by(|&a, &b| {
        customers[a]
            .arrival
            .cmp(&customers[b].arrival)
            .then_with(|| customers[a].priority.cmp(&customers[b].priority))
            .then_with(|| customers[b].id.cmp(&customers[a].id))
    });

    let jobs: Vec<Job> = customers
        .iter()
        .cloned()
        .map(|customer| Job {
            remaining: customer.service,
            customer,
            started: false,
            pause_time: None,
        })
        .collect();

    let mut jobs = jobs;
    let mut waiting: Vec<usize> = Vec::new();
    let mut running: Option<Running> = None;
    let mut last_dispatched: Option<u32> = None;
    let mut next_arrival = 0usize;
    let mut now = 0u64;
    let mut events = Vec::new();
    let mut finished_count = 0usize;

    while finished_count < customers.len() {
        let next_arrival_time = order
            .get(next_arrival)
            .map(|&i| jobs[i].customer.arrival);
        let next_finish = running.as_ref().map(|r| r.finish);
        let next_work = running.as_ref().map(|r| r.work_at).filter(|&t| t > now);

        let mut next_tick = None;
        for t in [next_arrival_time, next_finish, next_work]
            .into_iter()
            .flatten()
        {
            if t >= now && next_tick.map_or(true, |old| t < old) {
                next_tick = Some(t);
            }
        }
        let Some(tick) = next_tick else {
            break;
        };
        now = tick;

        if running.as_ref().is_some_and(|r| r.finish == now) {
            let r = running.take().unwrap();
            let id = jobs[r.index].customer.id;
            events.push(Event {
                time: now,
                customer: id,
                kind: Kind::Finish,
            });
            finished_count += 1;
        }

        while next_arrival < order.len()
            && jobs[order[next_arrival]].customer.arrival == now
        {
            let i = order[next_arrival];
            next_arrival += 1;
            events.push(Event {
                time: now,
                customer: jobs[i].customer.id,
                kind: Kind::Arrive,
            });
            waiting.push(i);
        }

        if let Some(r) = running.as_ref() {
            if now >= r.work_at {
                if let Some(w) = best_waiter(&waiting, &jobs) {
                    if jobs[w].customer.priority < jobs[r.index].customer.priority {
                        let r = running.take().unwrap();
                        let by = jobs[w].customer.id;
                        let remaining = r.finish - now;
                        jobs[r.index].remaining = remaining;
                        jobs[r.index].pause_time = Some(now);
                        events.push(Event {
                            time: now,
                            customer: jobs[r.index].customer.id,
                            kind: Kind::Preempt { remaining, by },
                        });
                        waiting.push(r.index);
                    }
                }
            }
        }

        while running.is_none() {
            let Some(w) = best_waiter(&waiting, &jobs) else {
                break;
            };
            waiting.retain(|&i| i != w);

            let id = jobs[w].customer.id;
            let cost = if last_dispatched == Some(id) {
                0
            } else if last_dispatched.is_none() {
                0
            } else {
                switch_cost
            };

            let work_at = now.checked_add(cost).ok_or(Error::TimeOverflow {
                customer: id,
                time: now,
            })?;
            let finish = work_at
                .checked_add(jobs[w].remaining)
                .ok_or(Error::TimeOverflow {
                    customer: id,
                    time: now,
                })?;

            let kind = if jobs[w].started {
                Kind::Resume {
                    remaining: jobs[w].remaining,
                    work_at,
                    finish,
                }
            } else {
                Kind::Start {
                    remaining: jobs[w].remaining,
                    work_at,
                    finish,
                }
            };
            jobs[w].started = true;
            jobs[w].pause_time = None;
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
                finished_count += 1;
            } else {
                running = Some(Running {
                    index: w,
                    work_at,
                    finish,
                });
            }
        }
    }

    Ok(events)
}