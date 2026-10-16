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
struct Running {
    job_index: usize,
    work_at: u64,
    finish: u64,
}

fn waiter_better(a: &Job, b: &Job) -> bool {
    let ap = a.customer.priority;
    let bp = b.customer.priority;
    if ap != bp {
        return ap < bp;
    }

    match (a.pause_time, b.pause_time) {
        (Some(at), Some(bt)) => {
            at < bt || (at == bt && a.customer.id < b.customer.id)
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
    use std::collections::HashSet;

    let mut ids = HashSet::new();
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

    let mut order: Vec<usize> = (0..customers.len()).collect();
    order.sort_by(|&a, &b| {
        customers[a]
            .arrival
            .cmp(&customers[b].arrival)
            .then_with(|| customers[a].priority.cmp(&customers[b].priority))
            .then_with(|| customers[b].id.cmp(&customers[a].id))
    });

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

    let mut trace = Vec::new();
    let mut waiting: Vec<usize> = Vec::new();
    let mut running: Option<Running> = None;
    let mut next_arrival = 0usize;
    let mut last_dispatched: Option<u32> = None;
    let mut now = 0u64;

    loop {
        if let Some(active) = running.as_ref() {
            if active.finish == now {
                let job_index = active.job_index;
                trace.push(Event {
                    time: now,
                    customer: jobs[job_index].customer.id,
                    kind: Kind::Finish,
                });
                running = None;
            }
        }

        let cohort_start = next_arrival;
        while next_arrival < order.len()
            && customers[order[next_arrival]].arrival == now
        {
            next_arrival += 1;
        }
        for &input_index in &order[cohort_start..next_arrival] {
            trace.push(Event {
                time: now,
                customer: customers[input_index].id,
                kind: Kind::Arrive,
            });
            waiting.push(input_index);
        }

        if let Some(active) = running.as_ref() {
            if now >= active.work_at {
                if let Some(best) = best_waiter(&waiting, &jobs) {
                    let running_index = active.job_index;
                    if jobs[best].customer.priority < jobs[running_index].customer.priority {
                        let by = jobs[best].customer.id;
                        let remaining = active.finish - now;
                        trace.push(Event {
                            time: now,
                            customer: jobs[running_index].customer.id,
                            kind: Kind::Preempt { remaining, by },
                        });
                        jobs[running_index].remaining = remaining;
                        jobs[running_index].pause_time = Some(now);
                        waiting.push(running_index);
                        running = None;
                    }
                }
            }
        }

        while running.is_none() {
            let Some(selected) = best_waiter(&waiting, &jobs) else {
                break;
            };
            waiting.retain(|&index| index != selected);

            let id = jobs[selected].customer.id;
            let cost = if last_dispatched == Some(id) || last_dispatched.is_none() {
                0
            } else {
                switch_cost
            };
            let work_at = now.checked_add(cost).ok_or(Error::TimeOverflow {
                customer: id,
                time: now,
            })?;
            let finish = work_at.checked_add(jobs[selected].remaining).ok_or(
                Error::TimeOverflow {
                    customer: id,
                    time: now,
                },
            )?;

            let kind = if jobs[selected].started {
                Kind::Resume {
                    remaining: jobs[selected].remaining,
                    work_at,
                    finish,
                }
            } else {
                Kind::Start {
                    remaining: jobs[selected].remaining,
                    work_at,
                    finish,
                }
            };
            jobs[selected].started = true;
            jobs[selected].pause_time = None;
            last_dispatched = Some(id);
            trace.push(Event {
                time: now,
                customer: id,
                kind,
            });

            if finish == now {
                trace.push(Event {
                    time: now,
                    customer: id,
                    kind: Kind::Finish,
                });
            } else {
                running = Some(Running {
                    job_index: selected,
                    work_at,
                    finish,
                });
            }
        }

        if next_arrival == order.len() && running.is_none() && waiting.is_empty() {
            break;
        }

        let mut next_time: Option<u64> = None;
        if next_arrival < order.len() {
            next_time = Some(customers[order[next_arrival]].arrival);
        }
        if let Some(active) = running.as_ref() {
            for t in [active.work_at, active.finish] {
                if t > now && next_time.is_none_or(|current| t < current) {
                    next_time = Some(t);
                }
            }
        }

        match next_time {
            Some(time) => now = time,
            None => break,
        }
    }

    Ok(trace)
}