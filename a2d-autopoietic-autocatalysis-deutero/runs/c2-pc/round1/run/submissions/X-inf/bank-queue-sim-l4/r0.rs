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
    Renege { waited: u64 },
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
pub enum OverflowStage {
    WorkAt,
    Finish,
    Patience,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Error {
    CapacityTooLarge { capacity: usize },
    TooManyCustomers { count: usize },
    ZeroId { index: usize },
    DuplicateId { index: usize, id: u32 },
    BadPriority { index: usize, value: u8 },
    DeadlineOverflow { index: usize, id: u32 },
    TimeOverflow { customer: u32, time: u64, stage: OverflowStage },
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

    Some(if remainder > complement || (remainder == complement && quotient % 2 == 1) {
        quotient + 1
    } else {
        quotient
    })
}

#[derive(Clone)]
struct Waiter {
    index: usize,
    entry: u64,
    deadline: Option<u64>,
    waited: u64,
    patience_left: Option<u64>,
    remaining: u64,
    paused: bool,
    pause_time: u64,
}

#[derive(Clone)]
struct Active {
    index: usize,
    remaining: u64,
    work_at: u64,
    finish: u64,
    dispatch: u64,
    waited: u64,
    patience_left: Option<u64>,
}

fn emit(events: &mut Vec<Event>, time: u64, customer: u32, kind: Kind) {
    events.push(Event { time, customer, kind });
}

fn better_waiter(a: &Waiter, b: &Waiter, customers: &[Customer]) -> bool {
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

fn best_waiter_index(waiters: &[Waiter], customers: &[Customer]) -> Option<usize> {
    let mut best = None;
    for i in 0..waiters.len() {
        if best.map_or(true, |j| better_waiter(&waiters[i], &waiters[j], customers)) {
            best = Some(i);
        }
    }
    best
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

    for (index, c) in customers.iter().enumerate() {
        if c.id == 0 {
            return Err(Error::ZeroId { index });
        }
        if customers[..index].iter().any(|earlier| earlier.id == c.id) {
            return Err(Error::DuplicateId { index, id: c.id });
        }
        if c.priority > 3 {
            return Err(Error::BadPriority {
                index,
                value: c.priority,
            });
        }
        if let Some(p) = c.patience {
            if c.arrival.checked_add(p).is_none() {
                return Err(Error::DeadlineOverflow { index, id: c.id });
            }
        }
    }

    let mut arrivals: Vec<usize> = (0..customers.len()).collect();
    arrivals.sort_by(|&a, &b| {
        customers[a]
            .arrival
            .cmp(&customers[b].arrival)
            .then_with(|| customers[a].priority.cmp(&customers[b].priority))
            .then_with(|| customers[b].id.cmp(&customers[a].id))
    });

    let mut events = Vec::new();
    let mut waiters: Vec<Waiter> = Vec::new();
    let mut active: Option<Active> = None;
    let mut next_arrival = 0usize;
    let mut last_dispatch: Option<u32> = None;
    let mut ever_dispatched = vec![false; customers.len()];

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

    let mut now = arrivals.first().map_or(0, |&i| customers[i].arrival);

    while next_arrival < arrivals.len() || active.is_some() || !waiters.is_empty() {
        let mut progressed = false;

        // Phase A: finish before expirations and arrivals.
        if active.as_ref().is_some_and(|a| a.finish == now) {
            let a = active.take().unwrap();
            let c = &customers[a.index];
            busy_ticks += (now - a.dispatch) as u128;
            emit(&mut events, now, c.id, Kind::Finish);
            completed += 1;
            turnaround_ticks += (now - c.arrival) as u128;
            completion_order.push(c.id);
            progressed = true;
        }

        // Phase B: expire customers who were already waiting at this tick.
        let mut expired = Vec::new();
        let mut i = 0;
        while i < waiters.len() {
            if waiters[i].deadline.is_some_and(|d| d <= now) {
                expired.push(waiters.remove(i));
            } else {
                i += 1;
            }
        }
        expired.sort_by(|a, b| {
            a.deadline
                .cmp(&b.deadline)
                .then_with(|| customers[a.index].id.cmp(&customers[b.index].id))
        });
        for w in expired {
            let c = &customers[w.index];
            let segment = now - w.entry;
            let total_waited = w.waited + segment;
            wait_ticks += segment as u128;
            emit(
                &mut events,
                now,
                c.id,
                Kind::Renege {
                    waited: total_waited,
                },
            );
            reneged += 1;
            progressed = true;
        }

        // Phase C: process the full arrival cohort without dispatching mid-cohort.
        while next_arrival < arrivals.len()
            && customers[arrivals[next_arrival]].arrival == now
        {
            let index = arrivals[next_arrival];
            next_arrival += 1;
            let c = &customers[index];

            emit(&mut events, now, c.id, Kind::Arrive);
            let limit = config.waiting_capacity + usize::from(active.is_none());
            if waiters.len() >= limit {
                emit(&mut events, now, c.id, Kind::Balk);
                balked += 1;
            } else {
                accepted += 1;
                waiters.push(Waiter {
                    index,
                    entry: now,
                    deadline: c.patience.map(|p| c.arrival + p),
                    waited: 0,
                    patience_left: c.patience,
                    remaining: c.service,
                    paused: false,
                    pause_time: 0,
                });
            }
            progressed = true;
        }

        // Phase D: compare before adding the paused customer back to the queue.
        if let Some(a) = active.as_ref() {
            if now >= a.work_at {
                if let Some(wi) = best_waiter_index(&waiters, customers) {
                    if customers[waiters[wi].index].priority
                        < customers[a.index].priority
                    {
                        let a = active.take().unwrap();
                        let paused_customer = &customers[a.index];
                        let by = customers[waiters[wi].index].id;
                        let remaining = a.finish - now;

                        busy_ticks += (now - a.dispatch) as u128;
                        emit(
                            &mut events,
                            now,
                            paused_customer.id,
                            Kind::Preempt { remaining, by },
                        );
                        preemptions += 1;

                        let deadline = match a.patience_left {
                            Some(budget) => Some(now.checked_add(budget).ok_or(
                                Error::TimeOverflow {
                                    customer: paused_customer.id,
                                    time: now,
                                    stage: OverflowStage::Patience,
                                },
                            )?),
                            None => None,
                        };

                        waiters.push(Waiter {
                            index: a.index,
                            entry: now,
                            deadline,
                            waited: a.waited,
                            patience_left: a.patience_left,
                            remaining,
                            paused: true,
                            pause_time: now,
                        });
                        progressed = true;
                    }
                }
            }
        }

        // Phase E: dispatch, including repeated inline zero-duration completions.
        loop {
            if active.is_some() {
                break;
            }

            let Some(wi) = best_waiter_index(&waiters, customers) else {
                break;
            };
            let w = waiters.remove(wi);
            let c = &customers[w.index];
            let segment = now - w.entry;
            let total_waited = w.waited + segment;
            wait_ticks += segment as u128;

            let patience_left = w.patience_left.map(|budget| budget - segment);
            let remaining = w.remaining;
            let switching = last_dispatch.is_some_and(|id| id != c.id);
            let cost = if switching { config.switch_cost } else { 0 };

            let work_at = now.checked_add(cost).ok_or(Error::TimeOverflow {
                customer: c.id,
                time: now,
                stage: OverflowStage::WorkAt,
            })?;
            let finish = work_at.checked_add(remaining).ok_or(Error::TimeOverflow {
                customer: c.id,
                time: now,
                stage: OverflowStage::Finish,
            })?;

            let kind = if ever_dispatched[w.index] {
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

            if !ever_dispatched[w.index] {
                started += 1;
                ever_dispatched[w.index] = true;
            }
            emit(&mut events, now, c.id, kind);
            last_dispatch = Some(c.id);
            progressed = true;

            if finish == now {
                emit(&mut events, now, c.id, Kind::Finish);
                completed += 1;
                turnaround_ticks += (now - c.arrival) as u128;
                completion_order.push(c.id);
                continue;
            }

            active = Some(Active {
                index: w.index,
                remaining,
                work_at,
                finish,
                dispatch: now,
                waited: total_waited,
                patience_left,
            });
            break;
        }

        // Phase F: expire customers newly queued during this tick.
        let mut expired = Vec::new();
        let mut i = 0;
        while i < waiters.len() {
            if waiters[i].deadline.is_some_and(|d| d <= now) {
                expired.push(waiters.remove(i));
            } else {
                i += 1;
            }
        }
        expired.sort_by(|a, b| {
            a.deadline
                .cmp(&b.deadline)
                .then_with(|| customers[a.index].id.cmp(&customers[b.index].id))
        });
        for w in expired {
            let c = &customers[w.index];
            let segment = now - w.entry;
            wait_ticks += segment as u128;
            emit(
                &mut events,
                now,
                c.id,
                Kind::Renege {
                    waited: w.waited + segment,
                },
            );
            reneged += 1;
            progressed = true;
        }

        if next_arrival >= arrivals.len() && active.is_none() && waiters.is_empty() {
            break;
        }

        // Jump to the next arrival, finish, work boundary, or waiting deadline.
        let mut next: Option<u64> = None;
        if next_arrival < arrivals.len() {
            next = Some(customers[arrivals[next_arrival]].arrival);
        }
        if let Some(a) = &active {
            for t in [Some(a.finish), (now < a.work_at).then_some(a.work_at)]
                .into_iter()
                .flatten()
            {
                next = Some(next.map_or(t, |n| n.min(t)));
            }
        }
        for w in &waiters {
            if let Some(d) = w.deadline.filter(|&d| d > now) {
                next = Some(next.map_or(d, |n| n.min(d)));
            }
        }

        if let Some(t) = next {
            if t > now {
                now = t;
                continue;
            }
        }
        if !progressed {
            break;
        }
    }

    let elapsed = events.last().map_or(0, |e| e.time);
    let mean_wait_milli = if accepted == 0 {
        0
    } else {
        rounded_ratio(1000u128 * wait_ticks, accepted as u128).unwrap()
    };
    let mean_turnaround_milli = if completed == 0 {
        0
    } else {
        rounded_ratio(1000u128 * turnaround_ticks, completed as u128).unwrap()
    };
    let utilization_bps = if elapsed == 0 {
        0
    } else {
        rounded_ratio(10000u128 * busy_ticks, elapsed as u128).unwrap() as u32
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