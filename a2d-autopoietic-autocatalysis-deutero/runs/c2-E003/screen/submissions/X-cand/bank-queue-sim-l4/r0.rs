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
    initial_deadline: Option<u64>,
    accepted: bool,
    terminal: bool,
    remaining: u64,
    has_started: bool,
    queue_entry: u64,
    remaining_patience: Option<u64>,
    deadline: Option<u64>,
    waited: u64,
    pause_time: Option<u64>,
}

#[derive(Clone)]
struct Running {
    index: usize,
    work_at: u64,
    finish: u64,
    interval_start: u64,
}

fn emit(events: &mut Vec<Event>, time: u64, customer: u32, kind: Kind) {
    events.push(Event { time, customer, kind });
}

fn waiter_cmp(a: usize, b: usize, states: &[CustomerState]) -> std::cmp::Ordering {
    let a = &states[a];
    let b = &states[b];
    a.customer
        .priority
        .cmp(&b.customer.priority)
        .then_with(|| match (a.pause_time, b.pause_time) {
            (Some(pa), Some(pb)) => pa.cmp(&pb).then_with(|| a.customer.id.cmp(&b.customer.id)),
            (Some(_), None) => std::cmp::Ordering::Less,
            (None, Some(_)) => std::cmp::Ordering::Greater,
            (None, None) => a
                .customer
                .arrival
                .cmp(&b.customer.arrival)
                .then_with(|| b.customer.id.cmp(&a.customer.id)),
        })
}

fn best_waiter(queue: &[usize], states: &[CustomerState]) -> Option<usize> {
    queue.iter().copied().min_by(|a, b| waiter_cmp(*a, *b, states))
}

fn expire_waiters(
    queue: &mut Vec<usize>,
    states: &mut [CustomerState],
    now: u64,
    events: &mut Vec<Event>,
    stats: &mut Stats,
) {
    let mut expired: Vec<usize> = queue
        .iter()
        .copied()
        .filter(|&i| states[i].deadline.is_some_and(|d| d <= now))
        .collect();
    expired.sort_by(|&a, &b| {
        states[a]
            .deadline
            .cmp(&states[b].deadline)
            .then_with(|| states[a].customer.id.cmp(&states[b].customer.id))
    });

    for i in expired {
        queue.retain(|&j| j != i);
        let segment = now - states[i].queue_entry;
        states[i].waited += segment;
        stats.wait_ticks += segment as u128;
        states[i].terminal = true;
        stats.reneged += 1;
        emit(
            events,
            now,
            states[i].customer.id,
            Kind::Renege {
                waited: states[i].waited,
            },
        );
    }
}

fn dispatch(
    i: usize,
    now: u64,
    last_dispatched: &mut Option<u32>,
    config: &Config,
    states: &mut [CustomerState],
    running: &mut Option<Running>,
    events: &mut Vec<Event>,
    stats: &mut Stats,
) -> Result<(), Error> {
    let id = states[i].customer.id;
    let segment = now - states[i].queue_entry;
    states[i].waited += segment;
    stats.wait_ticks += segment as u128;
    if let Some(budget) = states[i].remaining_patience.as_mut() {
        *budget -= segment;
    }

    let switch = if last_dispatched.is_some_and(|previous| previous != id) {
        config.switch_cost
    } else {
        0
    };
    let work_at = now.checked_add(switch).ok_or(Error::TimeOverflow {
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
    if states[i].has_started {
        emit(
            events,
            now,
            id,
            Kind::Resume {
                remaining,
                work_at,
                finish,
            },
        );
    } else {
        states[i].has_started = true;
        stats.started += 1;
        emit(
            events,
            now,
            id,
            Kind::Start {
                remaining,
                work_at,
                finish,
            },
        );
    }
    *last_dispatched = Some(id);
    *running = Some(Running {
        index: i,
        work_at,
        finish,
        interval_start: now,
    });
    Ok(())
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

    let mut states = Vec::with_capacity(customers.len());
    for (index, customer) in customers.iter().enumerate() {
        if customer.id == 0 {
            return Err(Error::ZeroId { index });
        }
        if customers[..index].iter().any(|previous| previous.id == customer.id) {
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
        let initial_deadline = match customer.patience {
            Some(patience) => Some(customer.arrival.checked_add(patience).ok_or(
                Error::DeadlineOverflow {
                    index,
                    id: customer.id,
                },
            )?),
            None => None,
        };
        states.push(CustomerState {
            customer: customer.clone(),
            initial_deadline,
            accepted: false,
            terminal: false,
            remaining: customer.service,
            has_started: false,
            queue_entry: customer.arrival,
            remaining_patience: customer.patience,
            deadline: initial_deadline,
            waited: 0,
            pause_time: None,
        });
    }

    let mut arrival_order: Vec<usize> = (0..states.len()).collect();
    arrival_order.sort_by(|&a, &b| {
        states[a]
            .customer
            .arrival
            .cmp(&states[b].customer.arrival)
            .then_with(|| states[a].customer.priority.cmp(&states[b].customer.priority))
            .then_with(|| states[b].customer.id.cmp(&states[a].customer.id))
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
    let mut queue: Vec<usize> = Vec::new();
    let mut running: Option<Running> = None;
    let mut last_dispatched = None;
    let mut arrival_pos = 0;
    let mut now = 0u64;
    let mut has_emitted = false;

    loop {
        let next_arrival = arrival_order
            .get(arrival_pos)
            .map(|&i| states[i].customer.arrival);

        if !has_emitted && next_arrival.is_none() {
            break;
        }

        if !has_emitted {
            now = next_arrival.unwrap();
        }

        // Phase A: finish at this tick.
        if running.as_ref().is_some_and(|r| r.finish == now) {
            let r = running.take().unwrap();
            let i = r.index;
            stats.busy_ticks += (now - r.interval_start) as u128;
            stats.completed += 1;
            stats.turnaround_ticks += (now - states[i].customer.arrival) as u128;
            stats.completion_order.push(states[i].customer.id);
            states[i].terminal = true;
            emit(&mut events, now, states[i].customer.id, Kind::Finish);
            has_emitted = true;
        }

        // Phase B: existing waiters expire.
        expire_waiters(&mut queue, &mut states, now, &mut events, &mut stats);

        // Phase C: arrivals in this cohort.
        while arrival_pos < arrival_order.len()
            && states[arrival_order[arrival_pos]].customer.arrival == now
        {
            let i = arrival_order[arrival_pos];
            arrival_pos += 1;
            let id = states[i].customer.id;
            emit(&mut events, now, id, Kind::Arrive);
            has_emitted = true;
            let gate = config.waiting_capacity + usize::from(running.is_none());
            if queue.len() < gate {
                states[i].accepted = true;
                stats.accepted += 1;
                states[i].queue_entry = now;
                states[i].deadline = states[i].initial_deadline;
                queue.push(i);
            } else {
                stats.balked += 1;
                states[i].terminal = true;
                emit(&mut events, now, id, Kind::Balk);
            }
        }

        // Phase D: preempt only after protection has ended.
        if let Some(r) = running.as_ref() {
            if now >= r.work_at {
                if let Some(waiter) = best_waiter(&queue, &states) {
                    if states[waiter].customer.priority < states[r.index].customer.priority {
                        let old = running.take().unwrap();
                        let paused = old.index;
                        let remaining = old.finish - now;
                        stats.busy_ticks += (now - old.interval_start) as u128;
                        states[paused].remaining = remaining;
                        emit(
                            &mut events,
                            now,
                            states[paused].customer.id,
                            Kind::Preempt {
                                remaining,
                                by: states[waiter].customer.id,
                            },
                        );
                        stats.preemptions += 1;

                        states[paused].queue_entry = now;
                        states[paused].pause_time = Some(now);
                        if let Some(budget) = states[paused].remaining_patience {
                            states[paused].deadline =
                                Some(now.checked_add(budget).ok_or(Error::TimeOverflow {
                                    customer: states[paused].customer.id,
                                    time: now,
                                    stage: OverflowStage::Patience,
                                })?);
                        } else {
                            states[paused].deadline = None;
                        }
                        queue.push(paused);
                        has_emitted = true;
                    }
                }
            }
        }

        // Phase E: dispatch as often as inline completions permit.
        loop {
            if running.is_some() {
                break;
            }
            let Some(i) = best_waiter(&queue, &states) else {
                break;
            };
            queue.retain(|&j| j != i);
            dispatch(
                i,
                now,
                &mut last_dispatched,
                config,
                &mut states,
                &mut running,
                &mut events,
                &mut stats,
            )?;
            has_emitted = true;

            if running.as_ref().is_some_and(|r| r.finish == now) {
                let r = running.take().unwrap();
                let done = r.index;
                stats.completed += 1;
                stats.turnaround_ticks += (now - states[done].customer.arrival) as u128;
                stats.completion_order.push(states[done].customer.id);
                states[done].terminal = true;
                emit(&mut events, now, states[done].customer.id, Kind::Finish);
            } else {
                break;
            }
        }

        // Phase F: expire customers queued during this tick.
        expire_waiters(&mut queue, &mut states, now, &mut events, &mut stats);

        if arrival_pos >= arrival_order.len() && queue.is_empty() && running.is_none() {
            break;
        }

        let mut next_time: Option<u64> = None;
        let mut consider = |t: u64| {
            if t > now && next_time.is_none_or(|current| t < current) {
                next_time = Some(t);
            }
        };
        if let Some(&i) = arrival_order.get(arrival_pos) {
            consider(states[i].customer.arrival);
        }
        if let Some(r) = running.as_ref() {
            consider(r.finish);
            if r.work_at > now {
                consider(r.work_at);
            }
        }
        for &i in &queue {
            if let Some(deadline) = states[i].deadline {
                consider(deadline);
            }
        }

        if let Some(t) = next_time {
            now = t;
            has_emitted = true;
        } else if let Some(&i) = arrival_order.get(arrival_pos) {
            now = states[i].customer.arrival;
            has_emitted = true;
        } else {
            break;
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
        rounded_ratio(stats.turnaround_ticks * 1000, stats.completed as u128).unwrap()
    };
    stats.utilization_bps = if stats.elapsed == 0 {
        0
    } else {
        rounded_ratio(stats.busy_ticks * 10_000, stats.elapsed as u128).unwrap() as u32
    };

    Ok(Report { events, stats })
}