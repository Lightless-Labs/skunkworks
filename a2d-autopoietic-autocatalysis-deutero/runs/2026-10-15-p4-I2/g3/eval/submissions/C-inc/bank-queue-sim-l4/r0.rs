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

    let round_up = if r > complement {
        true
    } else if r < complement {
        false
    } else {
        q % 2 == 1
    };

    Some(if round_up { q + 1 } else { q })
}

#[derive(Clone)]
struct State {
    customer: Customer,
    deadline: Option<u64>,
    entry: u64,
    waited: u128,
    remaining: u64,
    started: bool,
    paused_at: Option<u64>,
}

#[derive(Clone)]
struct Active {
    idx: usize,
    work_at: u64,
    finish: u64,
    dispatch: u64,
}

fn push(events: &mut Vec<Event>, time: u64, id: u32, kind: Kind) {
    events.push(Event { time, customer: id, kind });
}

fn best_waiter(waiting: &[usize], states: &[State]) -> Option<usize> {
    waiting.iter().copied().min_by(|&a, &b| {
        let x = &states[a];
        let y = &states[b];

        x.customer
            .priority
            .cmp(&y.customer.priority)
            .then_with(|| y.paused_at.is_some().cmp(&x.paused_at.is_some()))
            .then_with(|| match (x.paused_at, y.paused_at) {
                (Some(px), Some(py)) => px.cmp(&py).then(x.customer.id.cmp(&y.customer.id)),
                (None, None) => x.customer.arrival.cmp(&y.customer.arrival)
                    .then(y.customer.id.cmp(&x.customer.id)),
                _ => std::cmp::Ordering::Equal,
            })
    })
}

fn expiration_order(waiting: &[usize], states: &[State], now: u64) -> Vec<usize> {
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
    expired
}

fn expire(
    now: u64,
    waiting: &mut Vec<usize>,
    states: &mut [State],
    events: &mut Vec<Event>,
    stats: &mut Stats,
) {
    for i in expiration_order(waiting, states, now) {
        waiting.retain(|&x| x != i);

        let state = &mut states[i];
        let segment = now - state.entry;
        state.waited += segment as u128;
        stats.wait_ticks += segment as u128;

        push(
            events,
            now,
            state.customer.id,
            Kind::Renege {
                waited: state.waited as u64,
            },
        );
        stats.reneged += 1;
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
    for (index, customer) in customers.iter().enumerate() {
        if customer.id == 0 {
            return Err(Error::ZeroId { index });
        }
        if ids.contains(&customer.id) {
            return Err(Error::DuplicateId {
                index,
                id: customer.id,
            });
        }
        ids.push(customer.id);

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

    let mut states: Vec<State> = customers
        .iter()
        .cloned()
        .map(|customer| State {
            deadline: customer
                .patience
                .and_then(|p| customer.arrival.checked_add(p)),
            remaining: customer.service,
            entry: customer.arrival,
            customer,
            waited: 0,
            started: false,
            paused_at: None,
        })
        .collect();

    let mut events = Vec::new();
    let mut waiting = Vec::new();
    let mut active: Option<Active> = None;
    let mut next_arrival = 0usize;
    let mut now = 0u64;
    let mut last_dispatch: Option<u32> = None;

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

    while next_arrival < order.len() || active.is_some() || !waiting.is_empty() {
        if let Some(a) = active.as_ref() {
            now = now.max(a.dispatch);
        } else if next_arrival < order.len() {
            now = now.max(customers[order[next_arrival]].arrival);
        }

        // Phase A: finish at this tick before expirations or arrivals.
        if active.as_ref().is_some_and(|a| a.finish == now) {
            let a = active.take().unwrap();
            let state = &states[a.idx];

            stats.busy_ticks += (now - a.dispatch) as u128;
            push(&mut events, now, state.customer.id, Kind::Finish);
            stats.completed += 1;
            stats.turnaround_ticks += (now - state.customer.arrival) as u128;
            stats.completion_order.push(state.customer.id);
        }

        // Phase B.
        expire(now, &mut waiting, &mut states, &mut events, &mut stats);

        // Phase C: process the whole arrival cohort without dispatching.
        while next_arrival < order.len()
            && customers[order[next_arrival]].arrival == now
        {
            let i = order[next_arrival];
            next_arrival += 1;

            let customer = &customers[i];
            push(&mut events, now, customer.id, Kind::Arrive);

            let limit = config.waiting_capacity + usize::from(active.is_none());
            if waiting.len() >= limit {
                push(&mut events, now, customer.id, Kind::Balk);
                stats.balked += 1;
            } else {
                stats.accepted += 1;
                waiting.push(i);
            }
        }

        // Phase D: preempt only after switching protection ends.
        if let Some(a) = active.as_ref() {
            if now >= a.work_at {
                if let Some(w) = best_waiter(&waiting, &states) {
                    if states[w].customer.priority < states[a.idx].customer.priority {
                        let a = active.take().unwrap();
                        let remaining = a.finish - now;
                        let id = states[a.idx].customer.id;

                        push(
                            &mut events,
                            now,
                            id,
                            Kind::Preempt {
                                remaining,
                                by: states[w].customer.id,
                            },
                        );
                        stats.preemptions += 1;
                        stats.busy_ticks += (now - a.dispatch) as u128;

                        let deadline = match states[a.idx].customer.patience {
                            Some(budget) => Some(now.checked_add(budget).ok_or(
                                Error::TimeOverflow {
                                    customer: id,
                                    time: now,
                                    stage: OverflowStage::Patience,
                                },
                            )?),
                            None => None,
                        };

                        let state = &mut states[a.idx];
                        state.remaining = remaining;
                        state.entry = now;
                        state.deadline = deadline;
                        state.paused_at = Some(now);
                        waiting.push(a.idx);
                    }
                }
            }
        }

        // Phase E: dispatch until one customer remains active or no waiter remains.
        while active.is_none() {
            let Some(i) = best_waiter(&waiting, &states) else {
                break;
            };

            waiting.retain(|&x| x != i);

            let segment = now - states[i].entry;
            states[i].waited += segment as u128;
            stats.wait_ticks += segment as u128;

            let id = states[i].customer.id;
            let cost = if last_dispatch.is_some_and(|previous| previous != id) {
                config.switch_cost
            } else {
                0
            };

            let work_at = now.checked_add(cost).ok_or(Error::TimeOverflow {
                customer: id,
                time: now,
                stage: OverflowStage::WorkAt,
            })?;
            let finish = work_at
                .checked_add(states[i].remaining)
                .ok_or(Error::TimeOverflow {
                    customer: id,
                    time: now,
                    stage: OverflowStage::Finish,
                })?;

            let remaining = states[i].remaining;
            let kind = if states[i].started {
                Kind::Resume {
                    remaining,
                    work_at,
                    finish,
                }
            } else {
                states[i].started = true;
                stats.started += 1;
                Kind::Start {
                    remaining,
                    work_at,
                    finish,
                }
            };

            // Leaving the waiting queue consumes its current waiting deadline.
            states[i].deadline = None;
            states[i].paused_at = None;
            push(&mut events, now, id, kind);
            last_dispatch = Some(id);
            active = Some(Active {
                idx: i,
                work_at,
                finish,
                dispatch: now,
            });

            if finish == now {
                let a = active.take().unwrap();
                stats.busy_ticks += (now - a.dispatch) as u128;
                push(&mut events, now, id, Kind::Finish);
                stats.completed += 1;
                stats.turnaround_ticks += (now - states[i].customer.arrival) as u128;
                stats.completion_order.push(id);
            }
        }

        // Phase F.
        expire(now, &mut waiting, &mut states, &mut events, &mut stats);

        if next_arrival >= order.len() && active.is_none() && waiting.is_empty() {
            break;
        }

        let mut next: Option<u64> = None;
        let mut consider = |time: u64| {
            if time > now {
                next = Some(next.map_or(time, |current| current.min(time)));
            }
        };

        if next_arrival < order.len() {
            consider(customers[order[next_arrival]].arrival);
        }
        if let Some(a) = active.as_ref() {
            consider(a.finish);
            consider(a.work_at);
        }
        for &i in &waiting {
            if let Some(deadline) = states[i].deadline {
                consider(deadline);
            }
        }

        match next {
            Some(time) => now = time,
            None => break,
        }
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
        rounded_ratio(stats.busy_ticks * 10000, stats.elapsed as u128).unwrap() as u32
    };

    Ok(Report { events, stats })
}

fn main() {}