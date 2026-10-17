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
    remaining: u64,
    queue_since: u64,
    total_wait: u64,
    ever_started: bool,
    paused_at: Option<u64>,
}

#[derive(Clone)]
struct Running {
    index: usize,
    work_at: u64,
    finish: u64,
    protected: bool,
    dispatch_time: u64,
}

fn better_waiter(a: &CustomerState, b: &CustomerState) -> Ordering {
    a.customer
        .priority
        .cmp(&b.customer.priority)
        .then_with(|| match (a.paused_at, b.paused_at) {
            (Some(_), None) => Ordering::Less,
            (None, Some(_)) => Ordering::Greater,
            (Some(x), Some(y)) => x.cmp(&y).then(a.customer.id.cmp(&b.customer.id)),
            (None, None) => a
                .customer
                .arrival
                .cmp(&b.customer.arrival)
                .then(b.customer.id.cmp(&a.customer.id)),
        })
}

fn best_waiter(waiting: &[usize], states: &[CustomerState]) -> Option<usize> {
    waiting
        .iter()
        .copied()
        .min_by(|&a, &b| better_waiter(&states[a], &states[b]))
}

fn push_event(events: &mut Vec<Event>, time: u64, customer: u32, kind: Kind) {
    events.push(Event { time, customer, kind });
}

fn time_error(customer: u32, time: u64, stage: OverflowStage) -> Error {
    Error::TimeOverflow {
        customer,
        time,
        stage,
    }
}

fn expire_waiters(
    waiting: &mut Vec<usize>,
    states: &mut [CustomerState],
    events: &mut Vec<Event>,
    stats: &mut Stats,
    now: u64,
) {
    let mut expired: Vec<usize> = waiting
        .iter()
        .copied()
        .filter(|&i| states[i].deadline.is_some_and(|d| d <= now))
        .collect();

    expired.sort_by(|&a, &b| {
        states[a]
            .deadline
            .cmp(&states[b].deadline)
            .then(states[a].customer.id.cmp(&states[b].customer.id))
    });

    for i in expired {
        waiting.retain(|&x| x != i);
        let segment = now - states[i].queue_since;
        states[i].total_wait += segment;
        stats.wait_ticks += segment as u128;
        stats.reneged += 1;
        push_event(
            events,
            now,
            states[i].customer.id,
            Kind::Renege {
                waited: states[i].total_wait,
            },
        );
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

    let mut seen = Vec::with_capacity(customers.len());
    for (index, customer) in customers.iter().enumerate() {
        if customer.id == 0 {
            return Err(Error::ZeroId { index });
        }
        if seen.contains(&customer.id) {
            return Err(Error::DuplicateId {
                index,
                id: customer.id,
            });
        }
        seen.push(customer.id);
        if customer.priority > 3 {
            return Err(Error::BadPriority {
                index,
                value: customer.priority,
            });
        }
        if let Some(patience) = customer.patience {
            if customer.arrival.checked_add(patience).is_none() {
                return Err(Error::DeadlineOverflow {
                    index,
                    id: customer.id,
                });
            }
        }
    }

    let mut order: Vec<usize> = (0..customers.len()).collect();
    order.sort_by(|&a, &b| {
        customers[a]
            .arrival
            .cmp(&customers[b].arrival)
            .then(customers[a].priority.cmp(&customers[b].priority))
            .then(customers[b].id.cmp(&customers[a].id))
    });

    let mut states: Vec<CustomerState> = customers
        .iter()
        .cloned()
        .map(|customer| CustomerState {
            deadline: customer
                .patience
                .and_then(|p| customer.arrival.checked_add(p)),
            remaining: customer.service,
            queue_since: customer.arrival,
            total_wait: 0,
            ever_started: false,
            paused_at: None,
            customer,
        })
        .collect();

    let mut events = Vec::new();
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

    let mut now = 0u64;
    let mut next_arrival = 0usize;
    let mut waiting = Vec::<usize>::new();
    let mut running: Option<Running> = None;
    let mut last_dispatch = None::<u32>;
    let mut first_dispatch = true;

    while next_arrival < order.len()
        || running.is_some()
        || !waiting.is_empty()
    {
        let mut next_time = None::<u64>;
        let mut consider = |t: u64| {
            if t >= now {
                next_time = Some(next_time.map_or(t, |old| old.min(t)));
            }
        };

        if next_arrival < order.len() {
            consider(customers[order[next_arrival]].arrival);
        }
        if let Some(r) = &running {
            consider(r.finish);
            if r.protected && r.work_at > now {
                consider(r.work_at);
            }
        }
        for &i in &waiting {
            if let Some(deadline) = states[i].deadline {
                consider(deadline);
            }
        }

        let Some(t) = next_time else {
            break;
        };
        now = t;

        // Phase A: finish before expirations and arrivals.
        if running.as_ref().is_some_and(|r| r.finish == now) {
            let r = running.take().unwrap();
            let id = states[r.index].customer.id;
            stats.busy_ticks += (now - r.dispatch_time) as u128;
            stats.completed += 1;
            stats.turnaround_ticks +=
                (now - states[r.index].customer.arrival) as u128;
            stats.completion_order.push(id);
            push_event(&mut events, now, id, Kind::Finish);
        }

        // Phase B: expire customers that were already waiting.
        expire_waiters(&mut waiting, &mut states, &mut events, &mut stats, now);

        // Phase C: process the complete arrival cohort without dispatching.
        while next_arrival < order.len()
            && customers[order[next_arrival]].arrival == now
        {
            let i = order[next_arrival];
            next_arrival += 1;
            let id = states[i].customer.id;
            push_event(&mut events, now, id, Kind::Arrive);

            let limit =
                config.waiting_capacity + usize::from(running.is_none());
            if waiting.len() >= limit {
                stats.balked += 1;
                push_event(&mut events, now, id, Kind::Balk);
            } else {
                stats.accepted += 1;
                states[i].queue_since = now;
                waiting.push(i);
            }
        }

        // Phase D: preempt only after switching protection ends.
        if let Some(r) = running.clone() {
            if !r.protected || now >= r.work_at {
                if let Some(best) = best_waiter(&waiting, &states) {
                    if states[best].customer.priority
                        < states[r.index].customer.priority
                    {
                        let id = states[r.index].customer.id;
                        let remaining = r.finish - now;
                        push_event(
                            &mut events,
                            now,
                            id,
                            Kind::Preempt {
                                remaining,
                                by: states[best].customer.id,
                            },
                        );
                        stats.preemptions += 1;
                        stats.busy_ticks += (now - r.dispatch_time) as u128;
                        running = None;

                        let deadline = match states[r.index].customer.patience {
                            Some(budget) => Some(now.checked_add(budget).ok_or_else(
                                || time_error(id, now, OverflowStage::Patience),
                            )?),
                            None => None,
                        };
                        states[r.index].remaining = remaining;
                        states[r.index].queue_since = now;
                        states[r.index].paused_at = Some(now);
                        states[r.index].deadline = deadline;
                        waiting.push(r.index);
                    }
                }
            }
        }

        // Phase E: dispatch, finishing inline when finish equals now.
        loop {
            if running.is_some() {
                break;
            }
            let Some(i) = best_waiter(&waiting, &states) else {
                break;
            };
            waiting.retain(|&x| x != i);

            let id = states[i].customer.id;
            let waited = now - states[i].queue_since;
            states[i].total_wait += waited;
            stats.wait_ticks += waited as u128;

            let switch = if first_dispatch || last_dispatch == Some(id) {
                0
            } else {
                config.switch_cost
            };
            let work_at = now
                .checked_add(switch)
                .ok_or_else(|| time_error(id, now, OverflowStage::WorkAt))?;
            let finish = work_at
                .checked_add(states[i].remaining)
                .ok_or_else(|| time_error(id, now, OverflowStage::Finish))?;

            let kind = if states[i].ever_started {
                Kind::Resume {
                    remaining: states[i].remaining,
                    work_at,
                    finish,
                }
            } else {
                states[i].ever_started = true;
                stats.started += 1;
                Kind::Start {
                    remaining: states[i].remaining,
                    work_at,
                    finish,
                }
            };
            push_event(&mut events, now, id, kind);
            first_dispatch = false;
            last_dispatch = Some(id);
            running = Some(Running {
                index: i,
                work_at,
                finish,
                protected: work_at > now,
                dispatch_time: now,
            });

            if finish == now {
                let r = running.take().unwrap();
                stats.completed += 1;
                stats.turnaround_ticks +=
                    (now - states[r.index].customer.arrival) as u128;
                stats.completion_order.push(id);
                push_event(&mut events, now, id, Kind::Finish);
                continue;
            }
            break;
        }

        // Phase F: expire newly queued customers after dispatch opportunities.
        expire_waiters(&mut waiting, &mut states, &mut events, &mut stats, now);
    }

    stats.elapsed = events.last().map_or(0, |event| event.time);
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
        rounded_ratio(stats.busy_ticks * 10000, stats.elapsed as u128)
            .unwrap()
            .min(u32::MAX as u128) as u32
    };

    Ok(Report { events, stats })
}