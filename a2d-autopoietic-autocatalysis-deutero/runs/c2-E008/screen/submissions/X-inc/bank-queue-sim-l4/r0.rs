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
    let round_up = remainder > complement || (remainder == complement && quotient % 2 == 1);
    Some(if round_up { quotient + 1 } else { quotient })
}

#[derive(Clone)]
struct CustomerState {
    customer: Customer,
    initial_deadline: Option<u64>,
    accepted: bool,
    terminal: bool,
    started: bool,
    remaining: u64,
    queue_entry: u64,
    deadline: Option<u64>,
    waited: u64,
    last_preempt: Option<u64>,
}

#[derive(Clone, Copy)]
struct Active {
    index: usize,
    remaining: u64,
    work_at: u64,
    finish: u64,
    protected_until: u64,
    dispatched_at: u64,
}

fn emit(events: &mut Vec<Event>, time: u64, customer: u32, kind: Kind) {
    events.push(Event { time, customer, kind });
}

fn waiter_better(a: usize, b: usize, states: &[CustomerState]) -> bool {
    let sa = &states[a];
    let sb = &states[b];
    if sa.customer.priority != sb.customer.priority {
        return sa.customer.priority < sb.customer.priority;
    }

    match (sa.last_preempt, sb.last_preempt) {
        (Some(ta), Some(tb)) => ta < tb || (ta == tb && sa.customer.id < sb.customer.id),
        (Some(_), None) => true,
        (None, Some(_)) => false,
        (None, None) => {
            sa.customer.arrival < sb.customer.arrival
                || (sa.customer.arrival == sb.customer.arrival
                    && sa.customer.id > sb.customer.id)
        }
    }
}

fn best_waiter(waiting: &[usize], states: &[CustomerState]) -> Option<usize> {
    let mut best = None;
    for &i in waiting {
        if best.map_or(true, |b| waiter_better(i, b, states)) {
            best = Some(i);
        }
    }
    best
}

fn checked_time(
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

fn consume_wait(state: &mut CustomerState, now: u64) -> u64 {
    let segment = now - state.queue_entry;
    state.waited += segment;
    if let Some(deadline) = state.deadline {
        state.deadline = Some(deadline);
    }
    segment
}

fn expire_waiters(
    now: u64,
    waiting: &mut Vec<usize>,
    states: &mut [CustomerState],
    events: &mut Vec<Event>,
    stats: &mut Stats,
) {
    let mut expired: Vec<(u64, u32, usize)> = waiting
        .iter()
        .filter_map(|&i| {
            states[i]
                .deadline
                .filter(|&d| d <= now)
                .map(|d| (d, states[i].customer.id, i))
        })
        .collect();
    expired.sort_by_key(|&(deadline, id, _)| (deadline, id));

    for (_, _, i) in expired {
        if let Some(pos) = waiting.iter().position(|&x| x == i) {
            waiting.swap_remove(pos);
            let state = &mut states[i];
            consume_wait(state, now);
            state.terminal = true;
            stats.reneged += 1;
            emit(
                events,
                now,
                state.customer.id,
                Kind::Renege { waited: state.waited },
            );
        }
    }
}

fn dispatch(
    now: u64,
    waiting: &mut Vec<usize>,
    states: &mut [CustomerState],
    active: &mut Option<Active>,
    last_dispatched: &mut Option<u32>,
    config: &Config,
    events: &mut Vec<Event>,
    stats: &mut Stats,
) -> Result<(), Error> {
    let Some(i) = best_waiter(waiting, states) else {
        return Ok(());
    };
    let pos = waiting.iter().position(|&x| x == i).unwrap();
    waiting.swap_remove(pos);

    let id = states[i].customer.id;
    let segment = consume_wait(&mut states[i], now);
    stats.wait_ticks += segment as u128;
    if let Some(budget) = states[i].customer.patience {
        let remaining_budget = budget - segment;
        states[i].deadline = Some(now + remaining_budget);
    }

    let switch = if last_dispatched.is_some_and(|previous| previous != id) {
        config.switch_cost
    } else {
        0
    };
    let work_at = checked_time(id, now, switch, OverflowStage::WorkAt)?;
    let finish = checked_time(id, work_at, states[i].remaining, OverflowStage::Finish)?;
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
    emit(events, now, id, kind);

    *last_dispatched = Some(id);
    *active = Some(Active {
        index: i,
        remaining,
        work_at,
        finish,
        protected_until: work_at,
        dispatched_at: now,
    });

    if finish == now {
        let act = active.take().unwrap();
        stats.busy_ticks += (act.finish - act.dispatched_at) as u128;
        let state = &mut states[act.index];
        state.terminal = true;
        stats.completed += 1;
        stats.turnaround_ticks += (now - state.customer.arrival) as u128;
        stats.completion_order.push(state.customer.id);
        emit(events, now, state.customer.id, Kind::Finish);
    }
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

    let mut seen: Vec<u32> = Vec::with_capacity(customers.len());
    let mut states = Vec::with_capacity(customers.len());
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
        let initial_deadline = match customer.patience {
            Some(patience) => Some(
                customer
                    .arrival
                    .checked_add(patience)
                    .ok_or(Error::DeadlineOverflow {
                        index,
                        id: customer.id,
                    })?,
            ),
            None => None,
        };
        states.push(CustomerState {
            customer: customer.clone(),
            initial_deadline,
            accepted: false,
            terminal: false,
            started: false,
            remaining: customer.service,
            queue_entry: customer.arrival,
            deadline: initial_deadline,
            waited: 0,
            last_preempt: None,
        });
    }

    let mut order: Vec<usize> = (0..states.len()).collect();
    order.sort_by(|&a, &b| {
        let ca = &states[a].customer;
        let cb = &states[b].customer;
        ca.arrival
            .cmp(&cb.arrival)
            .then_with(|| ca.priority.cmp(&cb.priority))
            .then_with(|| cb.id.cmp(&ca.id))
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
    let mut last_dispatched = None;
    let mut next_arrival = 0;
    let mut now = 0u64;
    let mut have_time = false;

    while next_arrival < order.len() || active.is_some() || !waiting.is_empty() {
        let mut next = u64::MAX;
        let mut has_next = false;
        if let Some(&i) = order.get(next_arrival) {
            next = states[i].customer.arrival;
            has_next = true;
        }
        if let Some(act) = active {
            for t in [act.finish, act.work_at] {
                if t > now && (!has_next || t < next) {
                    next = t;
                    has_next = true;
                }
            }
        }
        for &i in &waiting {
            if let Some(deadline) = states[i].deadline {
                if deadline > now && (!has_next || deadline < next) {
                    next = deadline;
                    has_next = true;
                }
            }
        }
        if !has_next {
            if active.is_none() && waiting.is_empty() && next_arrival == order.len() {
                break;
            }
            break;
        }
        now = next;
        have_time = true;

        if let Some(act) = active {
            if act.finish == now {
                active = None;
                stats.busy_ticks += (now - act.dispatched_at) as u128;
                let state = &mut states[act.index];
                state.terminal = true;
                stats.completed += 1;
                stats.turnaround_ticks += (now - state.customer.arrival) as u128;
                stats.completion_order.push(state.customer.id);
                emit(&mut events, now, state.customer.id, Kind::Finish);
            }
        }

        expire_waiters(now, &mut waiting, &mut states, &mut events, &mut stats);

        while next_arrival < order.len()
            && states[order[next_arrival]].customer.arrival == now
        {
            let i = order[next_arrival];
            next_arrival += 1;
            let customer = &states[i].customer;
            emit(&mut events, now, customer.id, Kind::Arrive);
            let gate = config.waiting_capacity + usize::from(active.is_none());
            if waiting.len() >= gate {
                stats.balked += 1;
                emit(&mut events, now, customer.id, Kind::Balk);
            } else {
                let state = &mut states[i];
                state.accepted = true;
                state.queue_entry = now;
                state.deadline = state.initial_deadline;
                waiting.push(i);
                stats.accepted += 1;
            }
        }

        if let Some(act) = active {
            if now >= act.protected_until {
                if let Some(best) = best_waiter(&waiting, &states) {
                    if states[best].customer.priority < states[act.index].customer.priority {
                        let remaining = act.finish - now;
                        emit(
                            &mut events,
                            now,
                            states[act.index].customer.id,
                            Kind::Preempt {
                                remaining,
                                by: states[best].customer.id,
                            },
                        );
                        stats.preemptions += 1;
                        stats.busy_ticks += (now - act.dispatched_at) as u128;
                        active = None;
                        let state = &mut states[act.index];
                        state.remaining = remaining;
                        state.queue_entry = now;
                        state.last_preempt = Some(now);
                        if let Some(budget) = state.customer.patience {
                            state.deadline = Some(checked_time(
                                state.customer.id,
                                now,
                                budget - state.waited.min(budget),
                                OverflowStage::Patience,
                            )?);
                        }
                        waiting.push(act.index);
                    }
                }
            }
        }

        while active.is_none() && !waiting.is_empty() {
            dispatch(
                now,
                &mut waiting,
                &mut states,
                &mut active,
                &mut last_dispatched,
                config,
                &mut events,
                &mut stats,
            )?;
        }

        expire_waiters(now, &mut waiting, &mut states, &mut events, &mut stats);
    }

    if !have_time {
        now = 0;
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
        rounded_ratio(stats.busy_ticks * 10000, stats.elapsed as u128)
            .unwrap()
            .min(u32::MAX as u128) as u32
    };

    Ok(Report { events, stats })
}