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

struct Active {
    job: Job,
    work_at: u64,
    finish: u64,
}

fn waiter_better(a: &Job, b: &Job) -> bool {
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

fn best_waiter_index(waiting: &[Job]) -> Option<usize> {
    let mut best = None;
    for i in 0..waiting.len() {
        if best.is_none_or(|b| waiter_better(&waiting[i], &waiting[b])) {
            best = Some(i);
        }
    }
    best
}

pub fn simulate(customers: &[Customer], switch_cost: u64) -> Result<Vec<Event>, Error> {
    let mut ids = std::collections::HashSet::new();
    for (index, customer) in customers.iter().enumerate() {
        if customer.id == 0 {
            return Err(Error::ZeroId { index });
        }
        if !ids.insert(customer.id) {
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

    let mut arrivals: Vec<Customer> = customers.to_vec();
    arrivals.sort_by(|a, b| {
        a.arrival
            .cmp(&b.arrival)
            .then_with(|| a.priority.cmp(&b.priority))
            .then_with(|| b.id.cmp(&a.id))
    });

    let mut events = Vec::new();
    let mut waiting: Vec<Job> = Vec::new();
    let mut arrival_index = 0usize;
    let mut active: Option<Active> = None;
    let mut last_dispatched: Option<u32> = None;
    let mut finished_count = 0usize;
    let mut now = 0u64;

    while finished_count < customers.len() {
        // A scheduled finish occurs before arrivals and boundary decisions.
        if active.as_ref().is_some_and(|a| a.finish == now) {
            let a = active.take().unwrap();
            events.push(Event {
                time: now,
                customer: a.job.customer.id,
                kind: Kind::Finish,
            });
            finished_count += 1;
        }

        // Process the entire cohort before allowing dispatch or preemption.
        while arrival_index < arrivals.len() && arrivals[arrival_index].arrival == now {
            let customer = arrivals[arrival_index].clone();
            events.push(Event {
                time: now,
                customer: customer.id,
                kind: Kind::Arrive,
            });
            waiting.push(Job {
                remaining: customer.service,
                customer,
                started: false,
                pause_time: None,
            });
            arrival_index += 1;
        }

        // At and after work_at, an active job may be preempted by a
        // strictly higher-priority waiter.
        let can_preempt = active.as_ref().is_some_and(|a| now >= a.work_at && now < a.finish);
        if can_preempt {
            if let Some(wi) = best_waiter_index(&waiting) {
                let should_preempt = waiting[wi].customer.priority
                    < active.as_ref().unwrap().job.customer.priority;
                if should_preempt {
                    let by = waiting[wi].customer.id;
                    let mut a = active.take().unwrap();
                    a.job.remaining = a.finish - now;
                    a.job.pause_time = Some(now);
                    events.push(Event {
                        time: now,
                        customer: a.job.customer.id,
                        kind: Kind::Preempt {
                            remaining: a.job.remaining,
                            by,
                        },
                    });
                    waiting.push(a.job);
                }
            }
        }

        // Dispatch repeatedly at this tick, including zero-duration jobs.
        while active.is_none() {
            let Some(wi) = best_waiter_index(&waiting) else {
                break;
            };
            let mut job = waiting.remove(wi);
            let cost = if last_dispatched == Some(job.customer.id) {
                0
            } else if last_dispatched.is_none() {
                0
            } else {
                switch_cost
            };

            let work_at = now.checked_add(cost).ok_or(Error::TimeOverflow {
                customer: job.customer.id,
                time: now,
            })?;
            let finish = work_at.checked_add(job.remaining).ok_or(Error::TimeOverflow {
                customer: job.customer.id,
                time: now,
            })?;

            let kind = if job.started {
                Kind::Resume {
                    remaining: job.remaining,
                    work_at,
                    finish,
                }
            } else {
                Kind::Start {
                    remaining: job.remaining,
                    work_at,
                    finish,
                }
            };
            job.started = true;
            job.pause_time = None;
            last_dispatched = Some(job.customer.id);
            events.push(Event {
                time: now,
                customer: job.customer.id,
                kind,
            });

            if finish == now {
                events.push(Event {
                    time: now,
                    customer: job.customer.id,
                    kind: Kind::Finish,
                });
                finished_count += 1;
            } else {
                active = Some(Active {
                    job,
                    work_at,
                    finish,
                });
            }
        }

        if finished_count == customers.len() {
            break;
        }

        let mut next = None;
        if arrival_index < arrivals.len() {
            next = Some(arrivals[arrival_index].arrival);
        }
        if let Some(a) = active.as_ref() {
            for tick in [Some(a.finish), (a.work_at > now).then_some(a.work_at)]
                .into_iter()
                .flatten()
            {
                if tick > now && next.is_none_or(|n| tick < n) {
                    next = Some(tick);
                }
            }
        }

        if let Some(t) = next {
            now = t;
        } else {
            // There can be no unfinished work without an active job or
            // future arrival, but retain a safe termination path.
            break;
        }
    }

    Ok(events)
}