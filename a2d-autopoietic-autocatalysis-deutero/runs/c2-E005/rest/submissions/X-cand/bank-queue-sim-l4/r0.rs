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

    // Compare remainder with denominator - remainder, avoiding 2 * remainder.
    let round_up = remainder > complement
        || (remainder == complement && quotient % 2 == 1);

    if round_up {
        quotient.checked_add(1)
    } else {
        Some(quotient)
    }
}

#[derive(Clone)]
struct CustomerState {
    customer: Customer,
    deadline: Option<u64>,
    remaining: u64,
    ever_started: bool,
    waited: u64,
    queue_entry: u64,
    pause_time: Option<u64>,
    accepted: bool,
    terminal: bool,
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

fn waiting_better(a: usize, b: usize, states: &[CustomerState]) -> bool {
    let sa = &states[a];
    let sb = &states[b];
    let ca = &sa.customer;
    let cb = &sb.customer;

    if ca.priority != cb.priority {
        return ca.priority < cb.priority;
    }

    match (sa.pause_time, sb.pause_time) {
        (Some(ta), Some(tb)) => {
            ta < tb || (ta == tb && ca.id < cb.id)
        }
        (Some(_), None) => true,
        (None, Some(_)) => false,
        (None, None) => {
            ca.arrival < cb.arrival
                || (ca.arrival == cb.arrival && ca.id > cb.id)
        }
    }
}

fn best_waiter(waiting: &[usize], states: &[CustomerState]) -> Option<usize> {
    let mut best = None;
    for &candidate in waiting {
        if best.map_or(true, |current| waiting_better(candidate, current, states)) {
            best = Some(candidate);
        }
    }
    best
}

fn consume_wait(index: usize, now: u64, states: &mut [CustomerState], stats: &mut Stats) -> u64 {
    let segment = now - states[index].queue_entry;
    states[index].waited += segment;
    stats.wait_ticks += segment as u128;
    if let Some(deadline) = states[index].deadline {
        states[index].deadline = Some(deadline);
        // Remaining budget is tracked by moving the deadline after each consumed segment.
        states[index].deadline = Some(deadline);
    }
    segment
}

fn expire_waiters(
    waiting: &mut Vec<usize>,
    now: u64,
    states: &mut [CustomerState],
    events: &mut Vec<Event>,
    stats: &mut Stats,
) {
    let mut expired: Vec<(u64, u32, usize)> = waiting
        .iter()
        .filter_map(|&i| {
            states[i]
                .deadline
                .filter(|&deadline| deadline <= now)
                .map(|deadline| (deadline, states[i].customer.id, i))
        })
        .collect();
    expired.sort_by_key(|&(deadline, id, _)| (deadline, id));

    for (_, id, index) in expired {
        if let Some(position) = waiting.iter().position(|&x| x == index) {
            let segment = now - states[index].queue_entry;
            states[index].waited += segment;
            stats.wait_ticks += segment as u128;
            waiting.remove(position);
            states[index].terminal = true;
            stats.reneged += 1;
            emit(events, now, id, Kind::Renege { waited: states[index].waited });
        }
    }
}

pub fn simulate(customers: &[Customer], config: &Config) -> Result<Report, Error> {
    if config.waiting_capacity > 4096 {
        return Err(Error::CapacityTooLarge { capacity: config.waiting_capacity });
    }
    if customers.len() > 4096 {
        return Err(Error::TooManyCustomers { count: customers.len() });
    }

    let mut seen = std::collections::BTreeSet::new();
    let mut initial_deadlines = Vec::with_capacity(customers.len());
    for (index, customer) in customers.iter().enumerate() {
        if customer.id == 0 {
            return Err(Error::ZeroId { index });
        }
        if !seen.insert(customer.id) {
            return Err(Error::DuplicateId { index, id: customer.id });
        }
        if customer.priority > 3 {
            return Err(Error::BadPriority { index, value: customer.priority });
        }
        let deadline = match customer.patience {
            Some(patience) => Some(
                customer
                    .arrival
                    .checked_add(patience)
                    .ok_or(Error::DeadlineOverflow { index, id: customer.id })?,
            ),
            None => None,
        };
        initial_deadlines.push(deadline);
    }

    let mut states: Vec<CustomerState> = customers
        .iter()
        .zip(initial_deadlines)
        .map(|(customer, deadline)| CustomerState {
            customer: customer.clone(),
            deadline,
            remaining: customer.service,
            ever_started: false,
            waited: 0,
            queue_entry: customer.arrival,
            pause_time: None,
            accepted: false,
            terminal: false,
        })
        .collect();

    let mut arrival_order: Vec<usize> = (0..customers.len()).collect();
    arrival_order.sort_by(|&a, &b| {
        let ca = &states[a].customer;
        let cb = &states[b].customer;
        ca.arrival
            .cmp(&cb.arrival)
            .then(ca.priority.cmp(&cb.priority))
            .then(cb.id.cmp(&ca.id))
    });

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
    let mut waiting: Vec<usize> = Vec::new();
    let mut active: Option<Active> = None;
    let mut last_dispatched: Option<u32> = None;
    let mut arrival_cursor = 0usize;
    let mut now = arrival_order
        .first()
        .map(|&i| states[i].customer.arrival)
        .unwrap_or(0);

    while stats.completed + stats.reneged + stats.balked < customers.len() {
        // Phase A: finish the active customer before all other work at this tick.
        if let Some(current) = active.clone() {
            if current.finish == now {
                let index = current.index;
                let id = states[index].customer.id;
                stats.busy_ticks += (now - current.dispatch_time) as u128;
                stats.completed += 1;
                stats.turnaround_ticks += (now - states[index].customer.arrival) as u128;
                stats.completion_order.push(id);
                states[index].terminal = true;
                emit(&mut events, now, id, Kind::Finish);
                active = None;
            }
        }

        // Phase B: existing queued customers expire before arrivals and preemption.
        expire_waiters(&mut waiting, now, &mut states, &mut events, &mut stats);

        // Phase C: process the complete arrival cohort without dispatching.
        while arrival_cursor < arrival_order.len()
            && states[arrival_order[arrival_cursor]].customer.arrival == now
        {
            let index = arrival_order[arrival_cursor];
            arrival_cursor += 1;
            let id = states[index].customer.id;
            emit(&mut events, now, id, Kind::Arrive);
            let gate = config.waiting_capacity + usize::from(active.is_none());
            if waiting.len() >= gate {
                stats.balked += 1;
                states[index].terminal = true;
                emit(&mut events, now, id, Kind::Balk);
            } else {
                stats.accepted += 1;
                states[index].accepted = true;
                states[index].queue_entry = now;
                waiting.push(index);
            }
        }

        // Phase D: at and after the work boundary, consider one strict-priority preemption.
        if let Some(current) = active.clone() {
            if now >= current.work_at {
                if let Some(candidate) = best_waiter(&waiting, &states) {
                    if states[candidate].customer.priority < states[current.index].customer.priority {
                        let paused_index = current.index;
                        let paused_id = states[paused_index].customer.id;
                        let by = states[candidate].customer.id;
                        let remaining = current.finish - now;

                        emit(&mut events, now, paused_id, Kind::Preempt { remaining, by });
                        stats.preemptions += 1;
                        stats.busy_ticks += (now - current.dispatch_time) as u128;

                        states[paused_index].remaining = remaining;
                        states[paused_index].queue_entry = now;
                        states[paused_index].pause_time = Some(now);
                        if let Some(budget) = states[paused_index].customer.patience {
                            let spent = states[paused_index].waited;
                            let remaining_budget = budget.saturating_sub(spent);
                            states[paused_index].deadline = Some(
                                now.checked_add(remaining_budget).ok_or(Error::TimeOverflow {
                                    customer: paused_id,
                                    time: now,
                                    stage: OverflowStage::Patience,
                                })?,
                            );
                        }
                        waiting.push(paused_index);
                        active = None;
                    }
                }
            }
        }

        // Phase E: dispatch repeatedly after inline zero-duration completions.
        loop {
            if active.is_some() {
                break;
            }
            let Some(index) = best_waiter(&waiting, &states) else {
                break;
            };
            waiting.retain(|&x| x != index);

            let id = states[index].customer.id;
            let segment = now - states[index].queue_entry;
            states[index].waited += segment;
            stats.wait_ticks += segment as u128;
            if let Some(budget) = states[index].customer.patience {
                let spent = states[index].waited;
                let remaining_budget = budget.saturating_sub(spent);
                states[index].deadline = Some(now.saturating_add(remaining_budget));
            }

            let switch = if last_dispatched.map_or(false, |previous| previous != id) {
                config.switch_cost
            } else {
                0
            };
            let work_at = now.checked_add(switch).ok_or(Error::TimeOverflow {
                customer: id,
                time: now,
                stage: OverflowStage::WorkAt,
            })?;
            let finish = work_at.checked_add(states[index].remaining).ok_or(Error::TimeOverflow {
                customer: id,
                time: now,
                stage: OverflowStage::Finish,
            })?;

            let first = !states[index].ever_started;
            if first {
                states[index].ever_started = true;
                stats.started += 1;
                emit(&mut events, now, id, Kind::Start {
                    remaining: states[index].remaining,
                    work_at,
                    finish,
                });
            } else {
                emit(&mut events, now, id, Kind::Resume {
                    remaining: states[index].remaining,
                    work_at,
                    finish,
                });
            }

            last_dispatched = Some(id);
            active = Some(Active {
                index,
                work_at,
                finish,
                dispatch_time: now,
            });

            if finish == now {
                stats.busy_ticks += 0;
                stats.completed += 1;
                stats.turnaround_ticks += (now - states[index].customer.arrival) as u128;
                stats.completion_order.push(id);
                states[index].terminal = true;
                emit(&mut events, now, id, Kind::Finish);
                active = None;
                continue;
            }
            break;
        }

        // Phase F: expire all queued customers after dispatching.
        expire_waiters(&mut waiting, now, &mut states, &mut events, &mut stats);

        if stats.completed + stats.reneged + stats.balked >= customers.len() {
            break;
        }

        let mut next: Option<u64> = None;
        if arrival_cursor < arrival_order.len() {
            next = Some(states[arrival_order[arrival_cursor]].customer.arrival);
        }
        if let Some(current) = &active {
            for candidate in [Some(current.finish), Some(current.work_at)] {
                if let Some(time) = candidate.filter(|&time| time > now) {
                    next = Some(next.map_or(time, |old| old.min(time)));
                }
            }
        }
        for &index in &waiting {
            if let Some(deadline) = states[index].deadline.filter(|&time| time > now) {
                next = Some(next.map_or(deadline, |old| old.min(deadline)));
            }
        }

        if let Some(next_time) = next {
            now = next_time;
        } else {
            break;
        }
    }

    stats.elapsed = events.last().map_or(0, |event| event.time);
    stats.mean_wait_milli = if stats.accepted == 0 {
        0
    } else {
        rounded_ratio(stats.wait_ticks * 1000, stats.accepted as u128).unwrap_or(0)
    };
    stats.mean_turnaround_milli = if stats.completed == 0 {
        0
    } else {
        rounded_ratio(stats.turnaround_ticks * 1000, stats.completed as u128).unwrap_or(0)
    };
    stats.utilization_bps = if stats.elapsed == 0 {
        0
    } else {
        rounded_ratio(stats.busy_ticks * 10_000, stats.elapsed as u128)
            .unwrap_or(0)
            .min(u32::MAX as u128) as u32
    };

    Ok(Report { events, stats })
}