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
    let q = numerator / denominator;
    let r = numerator % denominator;
    let complement = denominator - r;
    if r < complement || (r == complement && q % 2 == 0) {
        Some(q)
    } else {
        q.checked_add(1)
    }
}

#[derive(Clone)]
struct CustomerState {
    customer: Customer,
    deadline: Option<u64>,
    remaining: u64,
    started: bool,
    accepted: bool,
    terminal: bool,
    queue_entry: u64,
    waited: u64,
    pause_time: Option<u64>,
}

#[derive(Clone)]
struct Active {
    index: usize,
    work_at: u64,
    finish: u64,
    interval_start: u64,
}

fn emit(events: &mut Vec<Event>, time: u64, id: u32, kind: Kind) {
    events.push(Event { time, customer: id, kind });
}

fn waiting_cmp(a: &CustomerState, b: &CustomerState) -> std::cmp::Ordering {
    use std::cmp::Ordering;
    a.customer
        .priority
        .cmp(&b.customer.priority)
        .then_with(|| match (a.pause_time, b.pause_time) {
            (Some(pa), Some(pb)) => pa
                .cmp(&pb)
                .then_with(|| a.customer.id.cmp(&b.customer.id)),
            (Some(_), None) => Ordering::Less,
            (None, Some(_)) => Ordering::Greater,
            (None, None) => a
                .customer
                .arrival
                .cmp(&b.customer.arrival)
                .then_with(|| b.customer.id.cmp(&a.customer.id)),
        })
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
        if let Some(p) = c.patience {
            if c.arrival.checked_add(p).is_none() {
                return Err(Error::DeadlineOverflow { index, id: c.id });
            }
        }
    }

    let mut states: Vec<CustomerState> = customers
        .iter()
        .cloned()
        .map(|customer| CustomerState {
            deadline: customer.patience.and_then(|p| customer.arrival.checked_add(p)),
            remaining: customer.service,
            customer,
            started: false,
            accepted: false,
            terminal: false,
            queue_entry: 0,
            waited: 0,
            pause_time: None,
        })
        .collect();

    let mut arrival_order: Vec<usize> = (0..states.len()).collect();
    arrival_order.sort_by(|&a, &b| {
        states[a]
            .customer
            .arrival
            .cmp(&states[b].customer.arrival)
            .then_with(|| states[a].customer.priority.cmp(&states[b].customer.priority))
            .then_with(|| states[b].customer.id.cmp(&states[a].customer.id))
    });

    let mut events = Vec::new();
    let mut waiting: Vec<usize> = Vec::new();
    let mut active: Option<Active> = None;
    let mut next_arrival = 0usize;
    let mut last_dispatch: Option<u32> = None;

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
    let mut elapsed = 0u64;

    while next_arrival < arrival_order.len() || active.is_some() || !waiting.is_empty() {
        let mut next_time: Option<u64> = None;
        let mut consider = |t: u64| {
            next_time = Some(next_time.map_or(t, |cur| cur.min(t)));
        };
        if next_arrival < arrival_order.len() {
            consider(states[arrival_order[next_arrival]].customer.arrival);
        }
        if let Some(a) = &active {
            consider(a.finish);
            if a.work_at > elapsed {
                consider(a.work_at);
            }
        }
        for &i in &waiting {
            if let Some(d) = states[i].deadline {
                consider(d);
            }
        }
        let now = next_time.expect("pending work has a next time");
        elapsed = now;

        // Phase A: finish active work before other events.
        if active.as_ref().is_some_and(|a| a.finish == now) {
            let a = active.take().unwrap();
            let s = &mut states[a.index];
            busy_ticks += (now - a.interval_start) as u128;
            s.terminal = true;
            completed += 1;
            turnaround_ticks += (now - s.customer.arrival) as u128;
            completion_order.push(s.customer.id);
            emit(&mut events, now, s.customer.id, Kind::Finish);
        }

        // Phase B: expire those waiting at the start of this tick.
        let mut expired: Vec<usize> = waiting
            .iter()
            .copied()
            .filter(|&i| states[i].deadline.is_some_and(|d| d <= now))
            .collect();
        expired.sort_by_key(|&i| (states[i].deadline.unwrap(), states[i].customer.id));
        for i in expired {
            if let Some(pos) = waiting.iter().position(|&x| x == i) {
                waiting.remove(pos);
                let s = &mut states[i];
                let segment = now - s.queue_entry;
                s.waited += segment;
                wait_ticks += segment as u128;
                s.terminal = true;
                reneged += 1;
                emit(
                    &mut events,
                    now,
                    s.customer.id,
                    Kind::Renege { waited: s.waited },
                );
            }
        }

        // Phase C: process the entire arrival cohort without dispatch.
        while next_arrival < arrival_order.len()
            && states[arrival_order[next_arrival]].customer.arrival == now
        {
            let i = arrival_order[next_arrival];
            next_arrival += 1;
            let id = states[i].customer.id;
            emit(&mut events, now, id, Kind::Arrive);
            let limit = config.waiting_capacity + usize::from(active.is_none());
            if waiting.len() >= limit {
                balked += 1;
                states[i].terminal = true;
                emit(&mut events, now, id, Kind::Balk);
            } else {
                accepted += 1;
                states[i].accepted = true;
                states[i].queue_entry = now;
                states[i].deadline = states[i]
                    .customer
                    .patience
                    .map(|p| now.checked_add(p).expect("validated deadline"));
                waiting.push(i);
            }
        }

        // Phase D: reconsider preemption after protection has ended.
        let protected = active.as_ref().is_some_and(|a| now < a.work_at);
        if !protected && !waiting.is_empty() {
            waiting.sort_by(|&a, &b| waiting_cmp(&states[a], &states[b]));
            if let Some(a) = active.as_ref() {
                let running_i = a.index;
                let best_i = waiting[0];
                if states[best_i].customer.priority < states[running_i].customer.priority {
                    let a = active.take().unwrap();
                    let remaining = a.finish - now;
                    busy_ticks += (now - a.interval_start) as u128;
                    let paused_id = states[running_i].customer.id;
                    let by = states[best_i].customer.id;
                    states[running_i].remaining = remaining;
                    states[running_i].queue_entry = now;
                    states[running_i].pause_time = Some(now);
                    if let Some(budget) = states[running_i].customer.patience {
                        let consumed = states[running_i].waited;
                        let remaining_budget = budget.saturating_sub(consumed);
                        states[running_i].deadline = Some(now.checked_add(remaining_budget).ok_or(
                            Error::TimeOverflow {
                                customer: paused_id,
                                time: now,
                                stage: OverflowStage::Patience,
                            },
                        )?);
                    }
                    waiting.push(running_i);
                    preemptions += 1;
                    emit(
                        &mut events,
                        now,
                        paused_id,
                        Kind::Preempt { remaining, by },
                    );
                }
            }
        }

        // Phase E: dispatch until occupied, processing inline completions.
        loop {
            if active.is_some() || waiting.is_empty() {
                break;
            }
            waiting.sort_by(|&a, &b| waiting_cmp(&states[a], &states[b]));
            let i = waiting.remove(0);
            let id = states[i].customer.id;
            let segment = now - states[i].queue_entry;
            states[i].waited += segment;
            wait_ticks += segment as u128;
            if let Some(budget) = states[i].customer.patience {
                let remaining_budget = budget.saturating_sub(states[i].waited);
                states[i].deadline = Some(now.saturating_add(remaining_budget));
            }

            let switch = if last_dispatch.is_some_and(|last| last != id) {
                config.switch_cost
            } else {
                0
            };
            let work_at = now.checked_add(switch).ok_or(Error::TimeOverflow {
                customer: id,
                time: now,
                stage: OverflowStage::WorkAt,
            })?;
            let finish = work_at.checked_add(states[i].remaining).ok_or(Error::TimeOverflow {
                customer: id,
                time: now,
                stage: OverflowStage::Finish,
            })?;

            let first = !states[i].started;
            if first {
                states[i].started = true;
                started += 1;
                emit(
                    &mut events,
                    now,
                    id,
                    Kind::Start {
                        remaining: states[i].remaining,
                        work_at,
                        finish,
                    },
                );
            } else {
                emit(
                    &mut events,
                    now,
                    id,
                    Kind::Resume {
                        remaining: states[i].remaining,
                        work_at,
                        finish,
                    },
                );
            }
            last_dispatch = Some(id);
            active = Some(Active {
                index: i,
                work_at,
                finish,
                interval_start: now,
            });

            if finish == now {
                active = None;
                states[i].terminal = true;
                completed += 1;
                turnaround_ticks += (now - states[i].customer.arrival) as u128;
                completion_order.push(id);
                emit(&mut events, now, id, Kind::Finish);
                continue;
            }
            break;
        }

        // Phase F: expire newly queued customers after dispatch exhaustion.
        let mut expired: Vec<usize> = waiting
            .iter()
            .copied()
            .filter(|&i| states[i].deadline.is_some_and(|d| d <= now))
            .collect();
        expired.sort_by_key(|&i| (states[i].deadline.unwrap(), states[i].customer.id));
        for i in expired {
            if let Some(pos) = waiting.iter().position(|&x| x == i) {
                waiting.remove(pos);
                let s = &mut states[i];
                let segment = now - s.queue_entry;
                s.waited += segment;
                wait_ticks += segment as u128;
                s.terminal = true;
                reneged += 1;
                emit(
                    &mut events,
                    now,
                    s.customer.id,
                    Kind::Renege { waited: s.waited },
                );
            }
        }
    }

    let mean_wait_milli = if accepted == 0 {
        0
    } else {
        rounded_ratio(wait_ticks * 1000, accepted as u128).unwrap_or(0)
    };
    let mean_turnaround_milli = if completed == 0 {
        0
    } else {
        rounded_ratio(turnaround_ticks * 1000, completed as u128).unwrap_or(0)
    };
    let utilization_bps = if elapsed == 0 {
        0
    } else {
        rounded_ratio(busy_ticks * 10000, elapsed as u128)
            .unwrap_or(0)
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