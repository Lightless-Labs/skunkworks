use std::cmp::Ordering;

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

    Some(match remainder.cmp(&complement) {
        Ordering::Less => quotient,
        Ordering::Greater => quotient + 1,
        Ordering::Equal if quotient % 2 == 0 => quotient,
        Ordering::Equal => quotient + 1,
    })
}

#[derive(Clone)]
struct Waiting {
    index: usize,
    entry: u64,
    deadline: Option<u64>,
    budget: Option<u64>,
    paused: bool,
    pause_time: u64,
    remaining: u64,
}

#[derive(Clone)]
struct Active {
    index: usize,
    remaining: u64,
    work_at: u64,
    finish: u64,
    dispatch_time: u64,
    budget: Option<u64>,
}

fn waiting_cmp(a: &Waiting, b: &Waiting, customers: &[Customer]) -> Ordering {
    let ca = &customers[a.index];
    let cb = &customers[b.index];

    ca.priority
        .cmp(&cb.priority)
        .then_with(|| match (a.paused, b.paused) {
            (true, false) => Ordering::Less,
            (false, true) => Ordering::Greater,
            (true, true) => a
                .pause_time
                .cmp(&b.pause_time)
                .then(ca.id.cmp(&cb.id)),
            (false, false) => ca
                .arrival
                .cmp(&cb.arrival)
                .then(cb.id.cmp(&ca.id)),
        })
}

fn add_time(
    customer: u32,
    time: u64,
    amount: u64,
    stage: OverflowStage,
) -> Result<u64, Error> {
    time.checked_add(amount).ok_or(Error::TimeOverflow {
        customer,
        time,
        stage,
    })
}

fn expire_waiters(
    waiting: &mut Vec<Waiting>,
    customers: &[Customer],
    total_waited: &mut [u64],
    stats: &mut Stats,
    events: &mut Vec<Event>,
    now: u64,
) -> bool {
    let mut expired: Vec<(usize, u64)> = waiting
        .iter()
        .enumerate()
        .filter_map(|(i, w)| w.deadline.filter(|&d| d <= now).map(|d| (i, d)))
        .collect();

    expired.sort_by(|&(ia, da), &(ib, db)| {
        da.cmp(&db)
            .then(customers[waiting[ia].index].id.cmp(&customers[waiting[ib].index].id))
    });

    let any = !expired.is_empty();
    for (wi, _) in expired.into_iter().rev() {
        let w = waiting.remove(wi);
        let c = &customers[w.index];
        let segment = now - w.entry;
        total_waited[w.index] += segment;
        stats.wait_ticks += segment as u128;
        events.push(Event {
            time: now,
            customer: c.id,
            kind: Kind::Renege {
                waited: total_waited[w.index],
            },
        });
        stats.reneged += 1;
    }
    any
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

    let mut ids = Vec::with_capacity(customers.len());
    let mut initial_deadlines = Vec::with_capacity(customers.len());

    for (index, c) in customers.iter().enumerate() {
        if c.id == 0 {
            return Err(Error::ZeroId { index });
        }
        if ids.contains(&c.id) {
            return Err(Error::DuplicateId { index, id: c.id });
        }
        ids.push(c.id);

        if c.priority > 3 {
            return Err(Error::BadPriority {
                index,
                value: c.priority,
            });
        }

        let deadline = match c.patience {
            Some(p) => Some(c.arrival.checked_add(p).ok_or(Error::DeadlineOverflow {
                index,
                id: c.id,
            })?),
            None => None,
        };
        initial_deadlines.push(deadline);
    }

    let n = customers.len();
    let mut arrivals: Vec<usize> = (0..n).collect();
    arrivals.sort_by(|&a, &b| {
        customers[a]
            .arrival
            .cmp(&customers[b].arrival)
            .then(customers[a].priority.cmp(&customers[b].priority))
            .then(customers[b].id.cmp(&customers[a].id))
    });

    let mut events = Vec::new();
    let mut waiting: Vec<Waiting> = Vec::new();
    let mut active: Option<Active> = None;
    let mut next_arrival = 0;
    let mut now = arrivals
        .first()
        .map(|&i| customers[i].arrival)
        .unwrap_or(0);
    let mut last_dispatched: Option<u32> = None;
    let mut ever_started = vec![false; n];
    let mut total_waited = vec![0u64; n];

    let mut stats = Stats {
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
        completion_order: Vec::new(),
    };

    let mut last_event_time = 0;

    while stats.completed + stats.reneged + stats.balked < n {
        // Phase A: completion takes precedence over all other events at this tick.
        if active.as_ref().is_some_and(|a| a.finish == now) {
            let a = active.take().unwrap();
            let c = &customers[a.index];

            stats.busy_ticks += (now - a.dispatch_time) as u128;
            events.push(Event {
                time: now,
                customer: c.id,
                kind: Kind::Finish,
            });
            stats.completed += 1;
            stats.turnaround_ticks += (now - c.arrival) as u128;
            stats.completion_order.push(c.id);
            last_event_time = now;
        }

        // Phase B: expire customers who were already waiting at the start of the tick.
        if expire_waiters(
            &mut waiting,
            customers,
            &mut total_waited,
            &mut stats,
            &mut events,
            now,
        ) {
            last_event_time = now;
        }

        // Phase C: process the complete arrival cohort without dispatch or preemption.
        while next_arrival < n && customers[arrivals[next_arrival]].arrival == now {
            let idx = arrivals[next_arrival];
            next_arrival += 1;
            let c = &customers[idx];

            events.push(Event {
                time: now,
                customer: c.id,
                kind: Kind::Arrive,
            });
            last_event_time = now;

            let limit = config.waiting_capacity + usize::from(active.is_none());
            if waiting.len() >= limit {
                events.push(Event {
                    time: now,
                    customer: c.id,
                    kind: Kind::Balk,
                });
                stats.balked += 1;
                last_event_time = now;
            } else {
                stats.accepted += 1;
                waiting.push(Waiting {
                    index: idx,
                    entry: now,
                    deadline: initial_deadlines[idx],
                    budget: c.patience,
                    paused: false,
                    pause_time: 0,
                    remaining: c.service,
                });
            }
        }

        // Phase D: an unprotected active customer may be preempted by a superior waiter.
        if let Some(a) = active.as_ref() {
            if now >= a.work_at && !waiting.is_empty() {
                let best = (0..waiting.len())
                    .min_by(|&i, &j| waiting_cmp(&waiting[i], &waiting[j], customers))
                    .unwrap();

                if customers[waiting[best].index].priority < customers[a.index].priority {
                    let a = active.take().unwrap();
                    let paused_id = customers[a.index].id;
                    let by = customers[waiting[best].index].id;
                    let remaining = a.finish - now;

                    events.push(Event {
                        time: now,
                        customer: paused_id,
                        kind: Kind::Preempt { remaining, by },
                    });
                    stats.preemptions += 1;
                    stats.busy_ticks += (now - a.dispatch_time) as u128;
                    last_event_time = now;

                    let deadline = match a.budget {
                        Some(budget) => Some(add_time(
                            paused_id,
                            now,
                            budget,
                            OverflowStage::Patience,
                        )?),
                        None => None,
                    };

                    waiting.push(Waiting {
                        index: a.index,
                        entry: now,
                        deadline,
                        budget: a.budget,
                        paused: true,
                        pause_time: now,
                        remaining,
                    });
                }
            }
        }

        // Phase E: dispatch while idle, including inline zero-duration completions.
        loop {
            if active.is_some() || waiting.is_empty() {
                break;
            }

            let best = (0..waiting.len())
                .min_by(|&i, &j| waiting_cmp(&waiting[i], &waiting[j], customers))
                .unwrap();
            let w = waiting.remove(best);
            let c = &customers[w.index];

            let segment = now - w.entry;
            total_waited[w.index] += segment;
            stats.wait_ticks += segment as u128;

            let remaining_budget = w.budget.map(|budget| budget - segment);
            let switch = if last_dispatched.is_some() && last_dispatched != Some(c.id) {
                config.switch_cost
            } else {
                0
            };
            let work_at = add_time(c.id, now, switch, OverflowStage::WorkAt)?;
            let finish = add_time(c.id, work_at, w.remaining, OverflowStage::Finish)?;

            let kind = if ever_started[w.index] {
                Kind::Resume {
                    remaining: w.remaining,
                    work_at,
                    finish,
                }
            } else {
                ever_started[w.index] = true;
                stats.started += 1;
                Kind::Start {
                    remaining: w.remaining,
                    work_at,
                    finish,
                }
            };

            last_dispatched = Some(c.id);
            events.push(Event {
                time: now,
                customer: c.id,
                kind,
            });
            last_event_time = now;

            if finish == now {
                events.push(Event {
                    time: now,
                    customer: c.id,
                    kind: Kind::Finish,
                });
                stats.completed += 1;
                stats.turnaround_ticks += (now - c.arrival) as u128;
                stats.completion_order.push(c.id);
                last_event_time = now;
            } else {
                active = Some(Active {
                    index: w.index,
                    remaining: w.remaining,
                    work_at,
                    finish,
                    dispatch_time: now,
                    budget: remaining_budget,
                });
            }
        }

        // Phase F: expire newly queued customers after dispatch has been exhausted.
        if expire_waiters(
            &mut waiting,
            customers,
            &mut total_waited,
            &mut stats,
            &mut events,
            now,
        ) {
            last_event_time = now;
        }

        if stats.completed + stats.reneged + stats.balked >= n {
            break;
        }

        let mut next_time: Option<u64> = None;
        if next_arrival < n {
            next_time = Some(customers[arrivals[next_arrival]].arrival);
        }

        if let Some(a) = active.as_ref() {
            next_time = Some(next_time.map_or(a.finish, |t| t.min(a.finish)));
            if a.work_at > now {
                next_time = Some(next_time.map_or(a.work_at, |t| t.min(a.work_at)));
            }
        }

        for w in &waiting {
            if let Some(d) = w.deadline {
                if d > now {
                    next_time = Some(next_time.map_or(d, |t| t.min(d)));
                }
            }
        }

        match next_time {
            Some(t) if t > now => now = t,
            _ => break,
        }
    }

    stats.elapsed = if events.is_empty() { 0 } else { last_event_time };
    stats.mean_wait_milli = if stats.accepted == 0 {
        0
    } else {
        rounded_ratio(stats.wait_ticks * 1000, stats.accepted as u128).unwrap()
    };
    stats.mean_turnaround_milli = if stats.completed == 0 {
        0
    } else {
        rounded_ratio(
            stats.turnaround_ticks * 1000,
            stats.completed as u128,
        )
        .unwrap()
    };
    stats.utilization_bps = if stats.elapsed == 0 {
        0
    } else {
        rounded_ratio(stats.busy_ticks * 10_000, stats.elapsed as u128)
            .unwrap()
            .min(u32::MAX as u128) as u32
    };

    Ok(Report { events, stats })
}