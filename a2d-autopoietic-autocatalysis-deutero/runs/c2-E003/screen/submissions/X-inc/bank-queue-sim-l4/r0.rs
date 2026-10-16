#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Customer {
    pub id: u32,
    pub arrival: u64,
    pub service: u64,
    pub priority: u8,
    pub patience: Option<u64>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Config {
    pub switch_cost: u64,
    pub waiting_capacity: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Kind {
    Arrive,
    Balk,
    Renege {
        waited: u64,
    },
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
pub enum OverflowStage {
    WorkAt,
    Finish,
    Patience,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Error {
    CapacityTooLarge {
        capacity: usize,
    },
    TooManyCustomers {
        count: usize,
    },
    ZeroId {
        index: usize,
    },
    DuplicateId {
        index: usize,
        id: u32,
    },
    BadPriority {
        index: usize,
        value: u8,
    },
    DeadlineOverflow {
        index: usize,
        id: u32,
    },
    TimeOverflow {
        customer: u32,
        time: u64,
        stage: OverflowStage,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Stats {
    pub accepted: usize,
    pub balked: usize,
    pub reneged: usize,
    pub completed: usize,
    pub started: usize,
    pub preemptions: usize,
    pub elapsed: u64,
    pub busy_ticks: u128,
    pub wait_ticks: u128,
    pub turnaround_ticks: u128,
    pub mean_wait_milli: u128,
    pub mean_turnaround_milli: u128,
    pub utilization_bps: u32,
    pub completion_order: Vec<u32>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Report {
    pub events: Vec<Event>,
    pub stats: Stats,
}

pub fn rounded_ratio(numerator: u128, denominator: u128) -> Option<u128> {
    if denominator == 0 {
        return None;
    }

    let quotient = numerator / denominator;
    let remainder = numerator % denominator;
    let complement = denominator - remainder;

    let round_up = if remainder > complement {
        true
    } else if remainder < complement {
        false
    } else {
        quotient % 2 == 1
    };

    Some(if round_up { quotient + 1 } else { quotient })
}

#[derive(Clone)]
struct Waiting {
    index: usize,
    entry: u64,
    deadline: Option<u64>,
    remaining: u64,
    budget: Option<u64>,
    paused: bool,
    pause_time: u64,
    waited: u64,
}

#[derive(Clone)]
struct Running {
    index: usize,
    remaining: u64,
    work_at: u64,
    finish: u64,
    dispatched_at: u64,
    budget: Option<u64>,
}

fn better(a: &Waiting, b: &Waiting, customers: &[Customer]) -> bool {
    let ca = &customers[a.index];
    let cb = &customers[b.index];

    if ca.priority != cb.priority {
        return ca.priority < cb.priority;
    }
    if a.paused != b.paused {
        return a.paused;
    }
    if a.paused {
        if a.pause_time != b.pause_time {
            return a.pause_time < b.pause_time;
        }
        ca.id < cb.id
    } else if ca.arrival != cb.arrival {
        ca.arrival < cb.arrival
    } else {
        ca.id > cb.id
    }
}

fn best_waiter(queue: &[Waiting], customers: &[Customer]) -> Option<usize> {
    let mut best = None;
    for i in 0..queue.len() {
        if best.map_or(true, |b| better(&queue[i], &queue[b], customers)) {
            best = Some(i);
        }
    }
    best
}

fn expire_waiters(
    queue: &mut Vec<Waiting>,
    now: u64,
    customers: &[Customer],
    events: &mut Vec<Event>,
    wait_ticks: &mut u128,
    reneged: &mut usize,
) {
    let mut expired: Vec<(u64, usize)> = queue
        .iter()
        .enumerate()
        .filter_map(|(i, w)| w.deadline.filter(|&d| d <= now).map(|d| (d, i)))
        .collect();

    expired.sort_by(|&(da, ia), &(db, ib)| {
        da.cmp(&db)
            .then_with(|| customers[queue[ia].index].id.cmp(&customers[queue[ib].index].id))
    });

    for (_, qi) in expired.into_iter().rev() {
        let w = queue.remove(qi);
        let segment = now - w.entry;
        let waited = w.waited + segment;
        *wait_ticks += segment as u128;
        events.push(Event {
            time: now,
            customer: customers[w.index].id,
            kind: Kind::Renege { waited },
        });
        *reneged += 1;
    }
}

pub fn simulate(customers: &[Customer], config: &Config) -> Result<Report, Error> {
    if config.waiting_capacity > 4096 {
        return Err(Error::CapacityTooLarge {
            capacity: config.waiting_capacity,
        });
    }
    if customers.len() > 4096 {
        return Err(Error::TooManyCustomers {
            count: customers.len(),
        });
    }

    for (i, c) in customers.iter().enumerate() {
        if c.id == 0 {
            return Err(Error::ZeroId { index: i });
        }
        if customers[..i].iter().any(|previous| previous.id == c.id) {
            return Err(Error::DuplicateId {
                index: i,
                id: c.id,
            });
        }
        if c.priority > 3 {
            return Err(Error::BadPriority {
                index: i,
                value: c.priority,
            });
        }
        if let Some(patience) = c.patience {
            if c.arrival.checked_add(patience).is_none() {
                return Err(Error::DeadlineOverflow {
                    index: i,
                    id: c.id,
                });
            }
        }
    }

    let mut arrival_order: Vec<usize> = (0..customers.len()).collect();
    arrival_order.sort_by(|&a, &b| {
        customers[a]
            .arrival
            .cmp(&customers[b].arrival)
            .then_with(|| customers[a].priority.cmp(&customers[b].priority))
            .then_with(|| customers[b].id.cmp(&customers[a].id))
    });

    let mut events = Vec::new();
    let mut queue: Vec<Waiting> = Vec::new();
    let mut running: Option<Running> = None;
    let mut arrival_pos = 0usize;
    let mut last_dispatched: Option<u32> = None;
    let mut first_dispatch = true;

    let mut accepted = 0usize;
    let mut balked = 0usize;
    let mut reneged = 0usize;
    let mut completed = 0usize;
    let mut started = 0usize;
    let mut preemptions = 0usize;
    let mut busy_ticks = 0u128;
    let mut wait_ticks = 0u128;
    let mut turnaround_ticks = 0u128;
    let mut completion_order = Vec::new();
    let mut started_once = vec![false; customers.len()];

    if customers.is_empty() {
        return Ok(Report {
            events,
            stats: Stats {
                accepted: 0,
                balked: 0,
                reneged: 0,
                completed: 0,
                started: 0,
                preemptions: 0,
                elapsed: 0,
                busy_ticks: 0,
                wait_ticks: 0,
                turnaround_ticks: 0,
                mean_wait_milli: 0,
                mean_turnaround_milli: 0,
                utilization_bps: 0,
                completion_order,
            },
        });
    }

    let mut now = customers[arrival_order[0]].arrival;

    loop {
        // Phase A: finish before any other events at this tick.
        if running.as_ref().is_some_and(|r| r.finish == now) {
            let r = running.take().unwrap();
            let c = &customers[r.index];
            events.push(Event {
                time: now,
                customer: c.id,
                kind: Kind::Finish,
            });
            busy_ticks += (now - r.dispatched_at) as u128;
            completed += 1;
            turnaround_ticks += (now - c.arrival) as u128;
            completion_order.push(c.id);
        }

        // Phase B: expire waiters present at the start of the tick.
        expire_waiters(
            &mut queue,
            now,
            customers,
            &mut events,
            &mut wait_ticks,
            &mut reneged,
        );

        // Phase C: process the full arrival cohort before dispatch or preemption.
        while arrival_pos < arrival_order.len()
            && customers[arrival_order[arrival_pos]].arrival == now
        {
            let idx = arrival_order[arrival_pos];
            arrival_pos += 1;
            let c = &customers[idx];

            events.push(Event {
                time: now,
                customer: c.id,
                kind: Kind::Arrive,
            });

            let admission_limit =
                config.waiting_capacity + usize::from(running.is_none());
            if queue.len() >= admission_limit {
                events.push(Event {
                    time: now,
                    customer: c.id,
                    kind: Kind::Balk,
                });
                balked += 1;
                continue;
            }

            accepted += 1;
            let budget = c.patience;
            let deadline = budget.map(|p| c.arrival + p);
            queue.push(Waiting {
                index: idx,
                entry: now,
                deadline,
                remaining: c.service,
                budget,
                paused: false,
                pause_time: 0,
                waited: 0,
            });
        }

        // Phase D: preempt only after switching protection ends.
        if let Some(r) = running.as_ref() {
            if now >= r.work_at {
                if let Some(bi) = best_waiter(&queue, customers) {
                    let waiter_priority = customers[queue[bi].index].priority;
                    let running_priority = customers[r.index].priority;

                    if waiter_priority < running_priority {
                        let r = running.take().unwrap();
                        let remaining = r.finish - now;
                        let paused_index = r.index;
                        let paused_id = customers[paused_index].id;
                        let by = customers[queue[bi].index].id;

                        events.push(Event {
                            time: now,
                            customer: paused_id,
                            kind: Kind::Preempt { remaining, by },
                        });
                        busy_ticks += (now - r.dispatched_at) as u128;
                        preemptions += 1;

                        let deadline = match r.budget {
                            Some(budget) => Some(now.checked_add(budget).ok_or(
                                Error::TimeOverflow {
                                    customer: paused_id,
                                    time: now,
                                    stage: OverflowStage::Patience,
                                },
                            )?),
                            None => None,
                        };

                        queue.push(Waiting {
                            index: paused_index,
                            entry: now,
                            deadline,
                            remaining,
                            budget: r.budget,
                            paused: true,
                            pause_time: now,
                            waited: 0,
                        });
                    }
                }
            }
        }

        // Phase E: dispatch until a customer remains active or no waiter remains.
        loop {
            if running.is_some() {
                break;
            }

            let Some(qi) = best_waiter(&queue, customers) else {
                break;
            };

            let mut w = queue.remove(qi);
            let c = &customers[w.index];
            let segment = now - w.entry;
            w.waited += segment;
            wait_ticks += segment as u128;
            if let Some(budget) = w.budget.as_mut() {
                *budget -= segment;
            }

            let switch = if first_dispatch {
                0
            } else if last_dispatched == Some(c.id) {
                0
            } else {
                config.switch_cost
            };

            let work_at = now.checked_add(switch).ok_or(Error::TimeOverflow {
                customer: c.id,
                time: now,
                stage: OverflowStage::WorkAt,
            })?;
            let finish = work_at.checked_add(w.remaining).ok_or(Error::TimeOverflow {
                customer: c.id,
                time: now,
                stage: OverflowStage::Finish,
            })?;

            let is_first = !started_once[w.index];
            if is_first {
                started_once[w.index] = true;
                started += 1;
            }

            events.push(Event {
                time: now,
                customer: c.id,
                kind: if is_first {
                    Kind::Start {
                        remaining: w.remaining,
                        work_at,
                        finish,
                    }
                } else {
                    Kind::Resume {
                        remaining: w.remaining,
                        work_at,
                        finish,
                    }
                },
            });

            first_dispatch = false;
            last_dispatched = Some(c.id);

            if finish == now {
                events.push(Event {
                    time: now,
                    customer: c.id,
                    kind: Kind::Finish,
                });
                completed += 1;
                turnaround_ticks += (now - c.arrival) as u128;
                completion_order.push(c.id);
                continue;
            }

            running = Some(Running {
                index: w.index,
                remaining: w.remaining,
                work_at,
                finish,
                dispatched_at: now,
                budget: w.budget,
            });
            break;
        }

        // Phase F: expire waiters newly queued or paused at this tick.
        expire_waiters(
            &mut queue,
            now,
            customers,
            &mut events,
            &mut wait_ticks,
            &mut reneged,
        );

        if arrival_pos == arrival_order.len() && running.is_none() && queue.is_empty() {
            break;
        }

        let mut next = None;
        let mut consider = |t: u64| {
            if t > now && next.map_or(true, |n| t < n) {
                next = Some(t);
            }
        };

        if arrival_pos < arrival_order.len() {
            consider(customers[arrival_order[arrival_pos]].arrival);
        }
        if let Some(r) = &running {
            consider(r.finish);
            if r.work_at > now {
                consider(r.work_at);
            }
        }
        for w in &queue {
            if let Some(deadline) = w.deadline {
                consider(deadline);
            }
        }

        if let Some(t) = next {
            now = t;
        } else {
            break;
        }
    }

    let elapsed = events.last().map(|event| event.time).unwrap_or(0);
    let mean_wait_milli = if accepted == 0 {
        0
    } else {
        rounded_ratio(wait_ticks * 1000, accepted as u128).unwrap()
    };
    let mean_turnaround_milli = if completed == 0 {
        0
    } else {
        rounded_ratio(turnaround_ticks * 1000, completed as u128).unwrap()
    };
    let utilization_bps = if elapsed == 0 {
        0
    } else {
        rounded_ratio(busy_ticks * 10000, elapsed as u128)
            .unwrap()
            .min(u32::MAX as u128) as u32
    };

    Ok(Report {
        events,
        stats: Stats {
            accepted,
            balked,
            reneged,
            completed,
            started,
            preemptions,
            elapsed,
            busy_ticks,
            wait_ticks,
            turnaround_ticks,
            mean_wait_milli,
            mean_turnaround_milli,
            utilization_bps,
            completion_order,
        },
    })
}