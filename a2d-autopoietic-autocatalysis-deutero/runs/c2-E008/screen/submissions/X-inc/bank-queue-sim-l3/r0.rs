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
    Start { remaining: u64, work_at: u64, finish: u64 },
    Resume { remaining: u64, work_at: u64, finish: u64 },
    Preempt { remaining: u64, by: u32 },
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

fn waiter_better(a: &Job, b: &Job) -> bool {
    if a.customer.priority != b.customer.priority {
        return a.customer.priority < b.customer.priority;
    }
    match (a.pause_time, b.pause_time) {
        (Some(_), None) => true,
        (None, Some(_)) => false,
        (Some(ta), Some(tb)) => {
            ta < tb || (ta == tb && a.customer.id < b.customer.id)
        }
        (None, None) => {
            a.customer.arrival < b.customer.arrival
                || (a.customer.arrival == b.customer.arrival
                    && a.customer.id > b.customer.id)
        }
    }
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

    let mut ordered: Vec<usize> = (0..customers.len()).collect();
    ordered.sort_by(|&a, &b| {
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
    let mut next_arrival = 0usize;
    let mut running: Option<Running> = None;
    let mut last_dispatched: Option<u32> = None;
    let mut now = 0u64;

    loop {
        let mut next_tick: Option<u64> = None;
        if next_arrival < ordered.len() {
            next_tick = Some(customers[ordered[next_arrival]].arrival);
        }
        if let Some(active) = &running {
            next_tick = Some(next_tick.map_or(active.finish, |t| t.min(active.finish)));
            if active.work_at > now {
                next_tick = Some(next_tick.map_or(active.work_at, |t| t.min(active.work_at)));
            }
        }
        let Some(tick) = next_tick else {
            break;
        };
        now = tick;

        if let Some(active) = &running {
            if active.finish == now {
                let index = active.index;
                trace.push(Event {
                    time: now,
                    customer: jobs[index].customer.id,
                    kind: Kind::Finish,
                });
                running = None;
            }
        }

        while next_arrival < ordered.len()
            && customers[ordered[next_arrival]].arrival == now
        {
            let index = ordered[next_arrival];
            trace.push(Event {
                time: now,
                customer: jobs[index].customer.id,
                kind: Kind::Arrive,
            });
            waiting.push(index);
            next_arrival += 1;
        }

        if let Some(active) = &running {
            if now >= active.work_at {
                let best = waiting
                    .iter()
                    .copied()
                    .reduce(|a, b| if waiter_better(&jobs[a], &jobs[b]) { a } else { b });
                if let Some(best_index) = best {
                    if jobs[best_index].customer.priority < jobs[active.index].customer.priority {
                        let active = running.take().unwrap();
                        jobs[active.index].remaining = active.finish - now;
                        jobs[active.index].pause_time = Some(now);
                        let by = jobs[best_index].customer.id;
                        trace.push(Event {
                            time: now,
                            customer: jobs[active.index].customer.id,
                            kind: Kind::Preempt {
                                remaining: jobs[active.index].remaining,
                                by,
                            },
                        });
                        waiting.push(active.index);
                    }
                }
            }
        }

        while running.is_none() && !waiting.is_empty() {
            let best_pos = (0..waiting.len())
                .reduce(|best, pos| {
                    if waiter_better(&jobs[waiting[pos]], &jobs[waiting[best]]) {
                        pos
                    } else {
                        best
                    }
                })
                .unwrap();
            let index = waiting.remove(best_pos);
            let id = jobs[index].customer.id;
            let cost = if last_dispatched.is_some_and(|last| last != id) {
                switch_cost
            } else {
                0
            };
            let work_at = now.checked_add(cost).ok_or(Error::TimeOverflow {
                customer: id,
                time: now,
            })?;
            let finish = work_at.checked_add(jobs[index].remaining).ok_or(
                Error::TimeOverflow {
                    customer: id,
                    time: now,
                },
            )?;

            let remaining = jobs[index].remaining;
            let first = !jobs[index].started;
            jobs[index].started = true;
            jobs[index].pause_time = None;
            last_dispatched = Some(id);

            trace.push(Event {
                time: now,
                customer: id,
                kind: if first {
                    Kind::Start {
                        remaining,
                        work_at,
                        finish,
                    }
                } else {
                    Kind::Resume {
                        remaining,
                        work_at,
                        finish,
                    }
                },
            });

            if finish == now {
                trace.push(Event {
                    time: now,
                    customer: id,
                    kind: Kind::Finish,
                });
                continue;
            }
            running = Some(Running {
                index,
                work_at,
                finish,
            });
        }
    }

    Ok(trace)
}