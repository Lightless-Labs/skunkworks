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
    paused_at: Option<u64>,
}

#[derive(Clone)]
struct Running {
    job: Job,
    work_at: u64,
    finish: u64,
}

fn waiter_cmp(a: &Job, b: &Job) -> std::cmp::Ordering {
    use std::cmp::Ordering;

    a.customer
        .priority
        .cmp(&b.customer.priority)
        .then_with(|| match (a.paused_at, b.paused_at) {
            (Some(a_time), Some(b_time)) => a_time.cmp(&b_time),
            (Some(_), None) => Ordering::Less,
            (None, Some(_)) => Ordering::Greater,
            (None, None) => a
                .customer
                .arrival
                .cmp(&b.customer.arrival)
                .then_with(|| b.customer.id.cmp(&a.customer.id)),
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

    let mut arrivals: Vec<Customer> = customers.to_vec();
    arrivals.sort_by(|a, b| {
        a.arrival
            .cmp(&b.arrival)
            .then_with(|| a.priority.cmp(&b.priority))
            .then_with(|| b.id.cmp(&a.id))
    });

    let mut trace = Vec::new();
    let mut waiting: Vec<Job> = Vec::new();
    let mut running: Option<Running> = None;
    let mut next_arrival = 0usize;
    let mut last_dispatched: Option<u32> = None;
    let mut now = 0u64;

    while next_arrival < arrivals.len() || running.is_some() || !waiting.is_empty() {
        // Process a scheduled completion before arrivals and work boundaries.
        if running.as_ref().is_some_and(|r| r.finish == now) {
            let done = running.take().unwrap();
            trace.push(Event {
                time: now,
                customer: done.job.customer.id,
                kind: Kind::Finish,
            });
        }

        // Emit the entire cohort before considering dispatch or preemption.
        while next_arrival < arrivals.len() && arrivals[next_arrival].arrival == now {
            let customer = arrivals[next_arrival].clone();
            trace.push(Event {
                time: now,
                customer: customer.id,
                kind: Kind::Arrive,
            });
            waiting.push(Job {
                remaining: customer.service,
                customer,
                started: false,
                paused_at: None,
            });
            next_arrival += 1;
        }

        if let Some(active) = running.as_ref() {
            if now >= active.work_at {
                let best = waiting
                    .iter()
                    .min_by(|a, b| waiter_cmp(a, b));
                if let Some(best) = best {
                    if best.customer.priority < active.job.customer.priority {
                        let by = best.customer.id;
                        let mut paused = running.take().unwrap();
                        let remaining = paused.finish - now;
                        trace.push(Event {
                            time: now,
                            customer: paused.job.customer.id,
                            kind: Kind::Preempt { remaining, by },
                        });
                        paused.job.remaining = remaining;
                        paused.job.paused_at = Some(now);
                        waiting.push(paused.job);
                    }
                }
            }
        }

        // Dispatch repeatedly at this tick, including inline zero-length jobs.
        while running.is_none() && !waiting.is_empty() {
            let best_index = (0..waiting.len())
                .min_by(|&a, &b| waiter_cmp(&waiting[a], &waiting[b]))
                .unwrap();
            let mut job = waiting.remove(best_index);

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
                job.started = true;
                Kind::Start {
                    remaining: job.remaining,
                    work_at,
                    finish,
                }
            };

            let id = job.customer.id;
            last_dispatched = Some(id);
            trace.push(Event {
                time: now,
                customer: id,
                kind,
            });

            running = Some(Running {
                job,
                work_at,
                finish,
            });

            if finish == now {
                running = None;
                trace.push(Event {
                    time: now,
                    customer: id,
                    kind: Kind::Finish,
                });
            }
        }

        if next_arrival >= arrivals.len() && running.is_none() && waiting.is_empty() {
            break;
        }

        let mut next_time = arrivals.get(next_arrival).map(|c| c.arrival);
        if let Some(active) = running.as_ref() {
            for candidate in [Some(active.finish), Some(active.work_at)]
                .into_iter()
                .flatten()
                .filter(|&t| t > now)
            {
                next_time = Some(next_time.map_or(candidate, |current| current.min(candidate)));
            }
        }

        if let Some(t) = next_time {
            now = t;
        } else {
            break;
        }
    }

    Ok(trace)
}