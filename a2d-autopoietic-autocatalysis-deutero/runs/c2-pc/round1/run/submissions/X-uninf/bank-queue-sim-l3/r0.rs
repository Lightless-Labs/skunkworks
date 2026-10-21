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

struct Running {
    index: usize,
    work_at: u64,
    finish: u64,
}

fn waiter_precedes(a: &Job, b: &Job) -> bool {
    let pa = a.customer.priority;
    let pb = b.customer.priority;
    if pa != pb {
        return pa < pb;
    }

    match (a.pause_time, b.pause_time) {
        (Some(ta), Some(tb)) => {
            ta < tb || (ta == tb && a.customer.id < b.customer.id)
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
    waiting.iter().copied().min_by(|&a, &b| {
        if waiter_precedes(&jobs[a], &jobs[b]) {
            std::cmp::Ordering::Less
        } else if waiter_precedes(&jobs[b], &jobs[a]) {
            std::cmp::Ordering::Greater
        } else {
            std::cmp::Ordering::Equal
        }
    })
}

pub fn simulate(customers: &[Customer], switch_cost: u64) -> Result<Vec<Event>, Error> {
    let mut seen = std::collections::HashSet::new();
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
    let mut trace = Vec::new();
    let mut waiting: Vec<usize> = Vec::new();
    let mut running: Option<Running> = None;
    let mut next_arrival = 0usize;
    let mut now = 0u64;
    let mut last_dispatched: Option<u32> = None;
    let mut completed = 0usize;

    while completed < jobs.len() {
        let mut next_tick: Option<u64> = None;
        if let Some(&idx) = order.get(next_arrival) {
            next_tick = Some(customers[idx].arrival);
        }
        if let Some(run) = &running {
            next_tick = Some(next_tick.map_or(run.finish, |t| t.min(run.finish)));
            if run.work_at > now {
                next_tick = Some(next_tick.map_or(run.work_at, |t| t.min(run.work_at)));
            }
        }

        now = next_tick.expect("unfinished simulation must have a pending event");

        if let Some(run) = &running {
            if run.finish == now {
                let idx = run.index;
                trace.push(Event {
                    time: now,
                    customer: jobs[idx].customer.id,
                    kind: Kind::Finish,
                });
                running = None;
                completed += 1;
            }
        }

        while next_arrival < order.len()
            && customers[order[next_arrival]].arrival == now
        {
            let idx = order[next_arrival];
            trace.push(Event {
                time: now,
                customer: jobs[idx].customer.id,
                kind: Kind::Arrive,
            });
            waiting.push(idx);
            next_arrival += 1;
        }

        if let Some(run) = &running {
            if now >= run.work_at {
                if let Some(waiter) = best_waiter(&waiting, &jobs) {
                    let current = run.index;
                    if jobs[waiter].customer.priority < jobs[current].customer.priority {
                        let remaining = run.finish - now;
                        let by = jobs[waiter].customer.id;
                        trace.push(Event {
                            time: now,
                            customer: jobs[current].customer.id,
                            kind: Kind::Preempt { remaining, by },
                        });
                        jobs[current].remaining = remaining;
                        jobs[current].pause_time = Some(now);
                        waiting.push(current);
                        running = None;
                    }
                }
            }
        }

        while running.is_none() {
            let Some(idx) = best_waiter(&waiting, &jobs) else {
                break;
            };
            waiting.retain(|&item| item != idx);

            let id = jobs[idx].customer.id;
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
            trace.push(Event {
                time: now,
                customer: id,
                kind,
            });
            jobs[idx].started = true;
            jobs[idx].pause_time = None;
            last_dispatched = Some(id);
            running = Some(Running {
                index: idx,
                work_at,
                finish,
            });

            if finish == now {
                trace.push(Event {
                    time: now,
                    customer: id,
                    kind: Kind::Finish,
                });
                running = None;
                completed += 1;
            }
        }
    }

    Ok(trace)
}