use std::collections::HashSet;

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
    let half = denominator / 2;

    let round_up = if denominator % 2 == 0 {
        remainder > half || (remainder == half && quotient % 2 == 1)
    } else {
        remainder > half
    };

    Some(if round_up { quotient + 1 } else { quotient })
}

#[derive(Clone)]
struct CustomerState {
    customer: Customer,
    deadline: Option<u64>,
    queue_entry: u64,
    waited: u64,
    paused: bool,
    pause_time: u64,
    ever_started: bool,
}

#[derive(Clone, Copy)]
struct Active {
    index: usize,
    work_at: u64,
    finish: u64,
    interval_start: u64,
}

fn waiting_precedes(a: &CustomerState, b: &CustomerState) -> bool {
    if a.customer.priority != b.customer.priority {
        return a.customer.priority < b.customer.priority;
    }
    if a.paused != b.paused {
        return a.paused;
    }
    if a.paused {
        if a.pause_time != b.pause_time {
            return a.pause_time < b.pause_time;
        }
        a.customer.id < b.customer.id
    } else if a.customer.arrival != b.customer.arrival {
        a.customer.arrival < b.customer.arrival
    } else {
        a.customer.id > b.customer.id
    }
}

fn best_waiter(queue: &[usize], states: &[CustomerState]) -> Option<usize> {
    queue.iter().copied().min_by(|&a, &b| {
        if waiting_precedes(&states[a], &states[b]) {
            std::cmp::Ordering::Less
        } else if waiting_precedes(&states[b], &states[a]) {
            std::cmp::Ordering::Greater
        } else {
            std::cmp::Ordering::Equal
        }
    })
}

fn emit(events: &mut Vec<Event>, time: u64, customer: u32, kind: Kind) {
    events.push(Event { time, customer, kind });
}

fn expire_waiters(
    now: u64,
    queue: &mut Vec<usize>,
    states: &mut [CustomerState],
    events: &mut Vec<Event>,
    stats: &mut Stats,
) {
    let mut expiring: Vec<(usize, u64)> = queue
        .iter()
        .filter_map(|&i| states[i].deadline.filter(|&d| d <= now).map(|d| (i, d)))
        .collect();

    expiring.sort_by(|&(ia, da), &(ib, db)| {
        da.cmp(&db)
            .then_with(|| states[ia].customer.id.cmp(&states[ib].customer.id))
    });

    for (index, _) in expiring {
        if let Some(pos) = queue.iter().position(|&i| i == index) {
            queue.remove(pos);
            let state = &mut states[index];
            let waited_now = now - state.queue_entry;
            state.waited += waited_now;
            stats.wait_ticks += waited_now as u128;
            stats.reneged += 1;
            emit(
                events,
                now,
                state.customer.id,
                Kind::Renege {
                    waited: state.waited,
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

    let mut ids = HashSet::new();
    for (index, c) in customers.iter().enumerate() {
        if c.id == 0 {
            return Err(Error::ZeroId { index });
        }
        if !ids.insert(c.id) {
            return Err(Error::DuplicateId { index, id: c.id });
        }
        if c.priority > 3 {
            return Err(Error::BadPriority {
                index,
                value: c.priority,
            });
        }
        if let Some(patience) = c.patience {
            if c.arrival.checked_add(patience).is_none() {
                return Err(Error::DeadlineOverflow {
                    index,
                    id: c.id,
                });
            }
        }
    }

    let mut states = Vec::with_capacity(customers.len());
    for c in customers {
        states.push(CustomerState {
            customer: c.clone(),
            deadline: c.patience.map(|p| c.arrival + p),
            queue_entry: c.arrival,
            waited: 0,
            paused: false,
            pause_time: 0,
            ever_started: false,
        });
    }

    let mut arrival_order: Vec<usize> = (0..customers.len()).collect();
    arrival_order.sort_by(|&a, &b| {
        states[a]
            .customer
            .arrival
            .cmp(&states[b].customer.arrival)
            .then_with(|| {
                states[a]
                    .customer
                    .priority
                    .cmp(&states[b].customer.priority)
            })
            .then_with(|| states[b].customer.id.cmp(&states[a].customer.id))
    });

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

    let mut queue: Vec<usize> = Vec::new();
    let mut active: Option<Active> = None;
    let mut arrival_pos = 0;
    let mut now = 0u64;
    let mut last_dispatch: Option<u32> = None;

    loop {
        // Phase A: completion.
        if active.map(|a| a.finish == now).unwrap_or(false) {
            let a = active.take().unwrap();
            let state = &states[a.index];
            stats.busy_ticks += (now - a.interval_start) as u128;
            emit(&mut events, now, state.customer.id, Kind::Finish);
            stats.completed += 1;
            stats.turnaround_ticks += (now - state.customer.arrival) as u128;
            stats.completion_order.push(state.customer.id);
        }

        // Phase B: expirations present at the start of the tick.
        expire_waiters(now, &mut queue, &mut states, &mut events, &mut stats);

        // Phase C: entire arrival cohort.
        while arrival_pos < arrival_order.len()
            && states[arrival_order[arrival_pos]].customer.arrival == now
        {
            let i = arrival_order[arrival_pos];
            arrival_pos += 1;
            let id = states[i].customer.id;
            emit(&mut events, now, id, Kind::Arrive);

            let limit = config.waiting_capacity + usize::from(active.is_none());
            if queue.len() >= limit {
                stats.balked += 1;
                emit(&mut events, now, id, Kind::Balk);
            } else {
                stats.accepted += 1;
                queue.push(i);
            }
        }

        // Phase D: consider one preemption once protection has ended.
        if let Some(a) = active {
            if now >= a.work_at {
                let best = best_waiter(&queue, &states);
                if let Some(w) = best {
                    if states[w].customer.priority < states[a.index].customer.priority {
                        let remaining = a.finish - now;
                        let paused_id = states[a.index].customer.id;
                        let by = states[w].customer.id;
                        emit(
                            &mut events,
                            now,
                            paused_id,
                            Kind::Preempt { remaining, by },
                        );
                        stats.preemptions += 1;
                        stats.busy_ticks += (now - a.interval_start) as u128;
                        active = None;

                        let state = &mut states[a.index];
                        state.queue_entry = now;
                        state.paused = true;
                        state.pause_time = now;
                        state.customer.service = remaining;
                        if let Some(old_deadline) = state.deadline {
                            let budget = old_deadline.saturating_sub(now);
                            state.deadline = Some(now.checked_add(budget).ok_or(
                                Error::TimeOverflow {
                                    customer: paused_id,
                                    time: now,
                                    stage: OverflowStage::Patience,
                                },
                            )?);
                        }
                        queue.push(a.index);
                    }
                }
            }
        }

        // Phase E: dispatch and process inline zero-duration finishes.
        loop {
            if active.is_some() {
                break;
            }
            let Some(i) = best_waiter(&queue, &states) else {
                break;
            };
            let pos = queue.iter().position(|&x| x == i).unwrap();
            queue.remove(pos);

            let waited_now = now - states[i].queue_entry;
            states[i].waited += waited_now;
            stats.wait_ticks += waited_now as u128;

            if let Some(deadline) = states[i].deadline {
                let budget = deadline.saturating_sub(states[i].queue_entry);
                states[i].deadline = Some(now.saturating_add(budget.saturating_sub(waited_now)));
            }

            let id = states[i].customer.id;
            let remaining = states[i].customer.service;
            let switching = last_dispatch.map(|prev| prev != id).unwrap_or(false);
            let cost = if switching { config.switch_cost } else { 0 };
            let work_at = now.checked_add(cost).ok_or(Error::TimeOverflow {
                customer: id,
                time: now,
                stage: OverflowStage::WorkAt,
            })?;
            let finish = work_at.checked_add(remaining).ok_or(Error::TimeOverflow {
                customer: id,
                time: now,
                stage: OverflowStage::Finish,
            })?;

            let was_started = states[i].ever_started;
            if was_started {
                emit(
                    &mut events,
                    now,
                    id,
                    Kind::Resume {
                        remaining,
                        work_at,
                        finish,
                    },
                );
            } else {
                emit(
                    &mut events,
                    now,
                    id,
                    Kind::Start {
                        remaining,
                        work_at,
                        finish,
                    },
                );
                states[i].ever_started = true;
                stats.started += 1;
            }
            last_dispatch = Some(id);

            if finish == now {
                emit(&mut events, now, id, Kind::Finish);
                stats.completed += 1;
                stats.turnaround_ticks += (now - states[i].customer.arrival) as u128;
                stats.completion_order.push(id);
                continue;
            }

            active = Some(Active {
                index: i,
                work_at,
                finish,
                interval_start: now,
            });
            break;
        }

        // Phase F: expirations of customers queued during this tick.
        expire_waiters(now, &mut queue, &mut states, &mut events, &mut stats);

        if arrival_pos == arrival_order.len() && active.is_none() && queue.is_empty() {
            break;
        }

        let mut next = None;
        let mut consider = |time: u64| {
            if time > now && next.map(|n| time < n).unwrap_or(true) {
                next = Some(time);
            }
        };

        if arrival_pos < arrival_order.len() {
            consider(states[arrival_order[arrival_pos]].customer.arrival);
        }
        if let Some(a) = active {
            consider(a.finish);
            if a.work_at > now {
                consider(a.work_at);
            }
        }
        for &i in &queue {
            if let Some(deadline) = states[i].deadline {
                consider(deadline);
            }
        }

        if let Some(next_time) = next {
            now = next_time;
        } else {
            break;
        }
    }

    stats.elapsed = events.last().map(|e| e.time).unwrap_or(0);
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