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
    let half = denominator / 2;
    let round_up = if denominator % 2 == 0 {
        remainder > half || (remainder == half && quotient % 2 == 1)
    } else {
        remainder > half
    };
    Some(quotient + u128::from(round_up))
}

#[derive(Clone)]
struct CustomerState {
    customer: Customer,
    deadline: Option<u64>,
    remaining_budget: Option<u64>,
    remaining: u64,
    ever_started: bool,
    total_waited: u64,
    queue_entry: u64,
    pause_time: Option<u64>,
}

#[derive(Clone)]
struct Active {
    index: usize,
    work_at: u64,
    finish: u64,
    dispatch_time: u64,
}

fn emit(events: &mut Vec<Event>, time: u64, customer: u32, kind: Kind) {
    events.push(Event { time, customer, kind });
}

fn waiter_cmp(a: &CustomerState, b: &CustomerState) -> std::cmp::Ordering {
    use std::cmp::Ordering;
    a.customer
        .priority
        .cmp(&b.customer.priority)
        .then_with(|| match (a.pause_time, b.pause_time) {
            (Some(pa), Some(pb)) => pa.cmp(&pb).then_with(|| a.customer.id.cmp(&b.customer.id)),
            (Some(_), None) => Ordering::Less,
            (None, Some(_)) => Ordering::Greater,
            (None, None) => a
                .customer
                .arrival
                .cmp(&b.customer.arrival)
                .then_with(|| b.customer.id.cmp(&a.customer.id)),
        })
}

fn best_waiter(states: &[CustomerState], waiting: &[usize]) -> Option<usize> {
    waiting
        .iter()
        .copied()
        .min_by(|&a, &b| waiter_cmp(&states[a], &states[b]))
}

fn expire_waiters(
    now: u64,
    states: &mut [CustomerState],
    waiting: &mut Vec<usize>,
    events: &mut Vec<Event>,
    stats: &mut Stats,
) {
    let mut expired: Vec<(u64, u32, usize)> = waiting
        .iter()
        .copied()
        .filter_map(|i| {
            states[i]
                .deadline
                .filter(|&d| d <= now)
                .map(|d| (d, states[i].customer.id, i))
        })
        .collect();
    expired.sort_by_key(|&(deadline, id, _)| (deadline, id));

    for (_, _, i) in expired {
        if let Some(pos) = waiting.iter().position(|&j| j == i) {
            waiting.remove(pos);
            let segment = now - states[i].queue_entry;
            states[i].remaining_budget = states[i].remaining_budget.map(|b| b - segment);
            states[i].total_waited += segment;
            stats.reneged += 1;
            stats.wait_ticks += segment as u128;
            emit(
                events,
                now,
                states[i].customer.id,
                Kind::Renege {
                    waited: states[i].total_waited,
                },
            );
        }
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
            Some(p) => Some(
                c.arrival
                    .checked_add(p)
                    .ok_or(Error::DeadlineOverflow { index, id: c.id })?,
            ),
            None => None,
        };
        initial_deadlines.push(deadline);
    }

    let mut order: Vec<usize> = (0..customers.len()).collect();
    order.sort_by(|&a, &b| {
        customers[a]
            .arrival
            .cmp(&customers[b].arrival)
            .then_with(|| customers[a].priority.cmp(&customers[b].priority))
            .then_with(|| customers[b].id.cmp(&customers[a].id))
    });

    let mut states: Vec<CustomerState> = customers
        .iter()
        .enumerate()
        .map(|(i, c)| CustomerState {
            customer: c.clone(),
            deadline: initial_deadlines[i],
            remaining_budget: c.patience,
            remaining: c.service,
            ever_started: false,
            total_waited: 0,
            queue_entry: c.arrival,
            pause_time: None,
        })
        .collect();

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
    let mut events = Vec::new();
    let mut waiting = Vec::<usize>::new();
    let mut active: Option<Active> = None;
    let mut previous_dispatch: Option<u32> = None;
    let mut first_dispatch = true;
    let mut arrival_pos = 0usize;
    let mut now = order
        .first()
        .map(|&i| customers[i].arrival)
        .unwrap_or(0);

    while arrival_pos < order.len() || active.is_some() || !waiting.is_empty() {
        if let Some(a) = active.as_ref() {
            if a.finish == now {
                let i = a.index;
                stats.busy_ticks += (now - a.dispatch_time) as u128;
                active = None;
                stats.completed += 1;
                stats.turnaround_ticks += (now - states[i].customer.arrival) as u128;
                stats.completion_order.push(states[i].customer.id);
                emit(&mut events, now, states[i].customer.id, Kind::Finish);
            }
        }

        expire_waiters(now, &mut states, &mut waiting, &mut events, &mut stats);

        while arrival_pos < order.len() && customers[order[arrival_pos]].arrival == now {
            let i = order[arrival_pos];
            arrival_pos += 1;
            let c = &customers[i];
            emit(&mut events, now, c.id, Kind::Arrive);
            let gate = config.waiting_capacity + usize::from(active.is_none());
            if waiting.len() >= gate {
                stats.balked += 1;
                emit(&mut events, now, c.id, Kind::Balk);
            } else {
                states[i].queue_entry = now;
                waiting.push(i);
                stats.accepted += 1;
            }
        }

        if let Some(a) = active.as_ref() {
            if now >= a.work_at {
                if let Some(w) = best_waiter(&states, &waiting) {
                    if states[w].customer.priority < states[a.index].customer.priority {
                        let i = a.index;
                        let remaining = a.finish - now;
                        emit(
                            &mut events,
                            now,
                            states[i].customer.id,
                            Kind::Preempt {
                                remaining,
                                by: states[w].customer.id,
                            },
                        );
                        stats.preemptions += 1;
                        stats.busy_ticks += (now - a.dispatch_time) as u128;
                        active = None;
                        states[i].remaining = remaining;
                        states[i].queue_entry = now;
                        states[i].pause_time = Some(now);
                        if let Some(budget) = states[i].remaining_budget {
                            let deadline = now.checked_add(budget).ok_or(Error::TimeOverflow {
                                customer: states[i].customer.id,
                                time: now,
                                stage: OverflowStage::Patience,
                            })?;
                            states[i].deadline = Some(deadline);
                        } else {
                            states[i].deadline = None;
                        }
                        waiting.push(i);
                    }
                }
            }
        }

        while active.is_none() {
            let Some(i) = best_waiter(&states, &waiting) else {
                break;
            };
            let pos = waiting.iter().position(|&j| j == i).unwrap();
            waiting.remove(pos);

            let segment = now - states[i].queue_entry;
            states[i].remaining_budget = states[i].remaining_budget.map(|b| b - segment);
            states[i].total_waited += segment;
            stats.wait_ticks += segment as u128;

            let switch = if first_dispatch {
                0
            } else if previous_dispatch == Some(states[i].customer.id) {
                0
            } else {
                config.switch_cost
            };
            let work_at = now.checked_add(switch).ok_or(Error::TimeOverflow {
                customer: states[i].customer.id,
                time: now,
                stage: OverflowStage::WorkAt,
            })?;
            let finish = work_at.checked_add(states[i].remaining).ok_or(Error::TimeOverflow {
                customer: states[i].customer.id,
                time: now,
                stage: OverflowStage::Finish,
            })?;

            let was_started = states[i].ever_started;
            let kind = if was_started {
                Kind::Resume {
                    remaining: states[i].remaining,
                    work_at,
                    finish,
                }
            } else {
                Kind::Start {
                    remaining: states[i].remaining,
                    work_at,
                    finish,
                }
            };
            emit(&mut events, now, states[i].customer.id, kind);
            if !was_started {
                stats.started += 1;
                states[i].ever_started = true;
            }
            first_dispatch = false;
            previous_dispatch = Some(states[i].customer.id);
            states[i].deadline = None;
            active = Some(Active {
                index: i,
                work_at,
                finish,
                dispatch_time: now,
            });

            if finish == now {
                active = None;
                stats.completed += 1;
                stats.turnaround_ticks += (now - states[i].customer.arrival) as u128;
                stats.completion_order.push(states[i].customer.id);
                emit(&mut events, now, states[i].customer.id, Kind::Finish);
            }
        }

        expire_waiters(now, &mut states, &mut waiting, &mut events, &mut stats);

        let mut next = None::<u64>;
        if arrival_pos < order.len() {
            next = Some(customers[order[arrival_pos]].arrival);
        }
        if let Some(a) = active.as_ref() {
            for t in [Some(a.work_at), Some(a.finish)].into_iter().flatten() {
                if t > now {
                    next = Some(next.map_or(t, |n| n.min(t)));
                }
            }
        }
        for &i in &waiting {
            if let Some(d) = states[i].deadline.filter(|&d| d > now) {
                next = Some(next.map_or(d, |n| n.min(d)));
            }
        }
        if let Some(t) = next {
            now = t;
        } else if active.is_none() && waiting.is_empty() && arrival_pos >= order.len() {
            break;
        } else {
            break;
        }
    }

    stats.elapsed = events.last().map(|e| e.time).unwrap_or(0);
    stats.mean_wait_milli = if stats.accepted == 0 {
        0
    } else {
        rounded_ratio(1000 * stats.wait_ticks, stats.accepted as u128).unwrap_or(0)
    };
    stats.mean_turnaround_milli = if stats.completed == 0 {
        0
    } else {
        rounded_ratio(
            1000 * stats.turnaround_ticks,
            stats.completed as u128,
        )
        .unwrap_or(0)
    };
    stats.utilization_bps = if stats.elapsed == 0 {
        0
    } else {
        rounded_ratio(10000 * stats.busy_ticks, stats.elapsed as u128)
            .unwrap_or(0)
            .min(u32::MAX as u128) as u32
    };

    Ok(Report { events, stats })
}