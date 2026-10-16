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

    // Compare the remainder with half the denominator without doubling it.
    let above_half = remainder > denominator / 2
        && (denominator % 2 == 0 || remainder != denominator / 2);
    let exact_half = denominator % 2 == 0 && remainder == denominator / 2;

    Some(if above_half || (exact_half && quotient % 2 == 1) {
        quotient + 1
    } else {
        quotient
    })
}

#[derive(Clone)]
struct CustomerState {
    customer: Customer,
    initial_deadline: Option<u64>,
    remaining: u64,
    waited: u64,
    wait_since: u64,
    budget: Option<u64>,
    deadline: Option<u64>,
    paused_at: Option<u64>,
}

#[derive(Clone)]
struct Active {
    index: usize,
    work_at: u64,
    finish: u64,
    interval_start: u64,
}

fn emit(events: &mut Vec<Event>, time: u64, customer: u32, kind: Kind) {
    events.push(Event { time, customer, kind });
}

fn better_waiter(a: usize, b: usize, states: &[CustomerState]) -> bool {
    let x = &states[a];
    let y = &states[b];

    if x.customer.priority != y.customer.priority {
        return x.customer.priority < y.customer.priority;
    }

    match (x.paused_at, y.paused_at) {
        (Some(xt), Some(yt)) => {
            xt < yt || (xt == yt && x.customer.id < y.customer.id)
        }
        (Some(_), None) => true,
        (None, Some(_)) => false,
        (None, None) => {
            x.customer.arrival < y.customer.arrival
                || (x.customer.arrival == y.customer.arrival
                    && x.customer.id > y.customer.id)
        }
    }
}

fn best_waiter(waiting: &[usize], states: &[CustomerState]) -> Option<usize> {
    waiting.iter().copied().reduce(|best, candidate| {
        if better_waiter(candidate, best, states) {
            candidate
        } else {
            best
        }
    })
}

fn error_time(customer: u32, time: u64, stage: OverflowStage) -> Error {
    Error::TimeOverflow {
        customer,
        time,
        stage,
    }
}

fn expire_waiters(
    waiting: &mut Vec<usize>,
    states: &mut [CustomerState],
    now: u64,
    events: &mut Vec<Event>,
    wait_ticks: &mut u128,
    reneged: &mut usize,
) {
    let mut expired: Vec<usize> = waiting
        .iter()
        .copied()
        .filter(|&i| states[i].deadline.is_some_and(|d| d <= now))
        .collect();

    expired.sort_by_key(|&i| (states[i].deadline.unwrap(), states[i].customer.id));

    for i in expired {
        if let Some(pos) = waiting.iter().position(|&x| x == i) {
            waiting.remove(pos);
            let final_wait = now - states[i].wait_since;
            states[i].waited += final_wait;
            *wait_ticks += final_wait as u128;

            emit(
                events,
                now,
                states[i].customer.id,
                Kind::Renege {
                    waited: states[i].waited,
                },
            );
            *reneged += 1;
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

    let mut ids = Vec::<u32>::with_capacity(customers.len());
    let mut initial_deadlines = Vec::with_capacity(customers.len());

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
        if customer.priority > 3 {
            return Err(Error::BadPriority {
                index,
                value: customer.priority,
            });
        }

        let deadline = match customer.patience {
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

        ids.push(customer.id);
        initial_deadlines.push(deadline);
    }

    let mut states: Vec<CustomerState> = customers
        .iter()
        .enumerate()
        .map(|(i, customer)| CustomerState {
            customer: customer.clone(),
            initial_deadline: initial_deadlines[i],
            remaining: customer.service,
            waited: 0,
            wait_since: customer.arrival,
            budget: customer.patience,
            deadline: None,
            paused_at: None,
        })
        .collect();

    let mut arrivals: Vec<usize> = (0..customers.len()).collect();
    arrivals.sort_by(|&a, &b| {
        let x = &states[a].customer;
        let y = &states[b].customer;
        x.arrival
            .cmp(&y.arrival)
            .then(x.priority.cmp(&y.priority))
            .then(y.id.cmp(&x.id))
    });

    let mut events = Vec::new();
    let mut waiting = Vec::<usize>::new();
    let mut active: Option<Active> = None;
    let mut arrival_pos = 0usize;
    let mut now = 0u64;
    let mut last_dispatched: Option<u32> = None;

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

    while arrival_pos < arrivals.len() || active.is_some() || !waiting.is_empty() {
        if active.is_none() && waiting.is_empty() {
            if arrival_pos >= arrivals.len() {
                break;
            }
            now = now.max(states[arrivals[arrival_pos]].customer.arrival);
        }

        // Phase A: finish before expirations, arrivals, or preemption.
        if active.as_ref().is_some_and(|a| a.finish == now) {
            let a = active.take().unwrap();
            busy_ticks += (now - a.interval_start) as u128;

            let id = states[a.index].customer.id;
            emit(&mut events, now, id, Kind::Finish);
            completed += 1;
            turnaround_ticks += (now - states[a.index].customer.arrival) as u128;
            completion_order.push(id);
        }

        // Phase B: expire customers already waiting at the start of the tick.
        expire_waiters(
            &mut waiting,
            &mut states,
            now,
            &mut events,
            &mut wait_ticks,
            &mut reneged,
        );

        // Phase C: process the entire arrival cohort without dispatching.
        while arrival_pos < arrivals.len()
            && states[arrivals[arrival_pos]].customer.arrival == now
        {
            let i = arrivals[arrival_pos];
            arrival_pos += 1;

            let id = states[i].customer.id;
            emit(&mut events, now, id, Kind::Arrive);

            let gate = config.waiting_capacity + usize::from(active.is_none());
            if waiting.len() >= gate {
                emit(&mut events, now, id, Kind::Balk);
                balked += 1;
                continue;
            }

            states[i].wait_since = now;
            states[i].deadline = states[i].initial_deadline;
            waiting.push(i);
            accepted += 1;
        }

        // Phase D: preempt only after the active customer's protection ends.
        if let Some(a) = active.as_ref() {
            if now >= a.work_at {
                if let Some(w) = best_waiter(&waiting, &states) {
                    if states[w].customer.priority < states[a.index].customer.priority {
                        let a = active.take().unwrap();
                        busy_ticks += (now - a.interval_start) as u128;

                        let id = states[a.index].customer.id;
                        let by = states[w].customer.id;
                        let remaining = a.finish - now;
                        emit(
                            &mut events,
                            now,
                            id,
                            Kind::Preempt { remaining, by },
                        );
                        preemptions += 1;

                        let deadline = match states[a.index].budget {
                            Some(budget) => Some(now.checked_add(budget).ok_or_else(|| {
                                error_time(id, now, OverflowStage::Patience)
                            })?),
                            None => None,
                        };

                        states[a.index].remaining = remaining;
                        states[a.index].wait_since = now;
                        states[a.index].deadline = deadline;
                        states[a.index].paused_at = Some(now);
                        waiting.push(a.index);
                    }
                }
            }
        }

        // Phase E: dispatch until one customer remains active or no waiter remains.
        loop {
            if active.is_some() {
                break;
            }

            let Some(i) = best_waiter(&waiting, &states) else {
                break;
            };
            waiting.retain(|&x| x != i);

            let segment = now - states[i].wait_since;
            states[i].waited += segment;
            wait_ticks += segment as u128;

            if let Some(budget) = states[i].budget {
                states[i].budget = Some(budget - segment);
            }

            let id = states[i].customer.id;
            let switch = if last_dispatched.is_some_and(|previous| previous != id) {
                config.switch_cost
            } else {
                0
            };

            let work_at = now
                .checked_add(switch)
                .ok_or_else(|| error_time(id, now, OverflowStage::WorkAt))?;
            let finish = work_at
                .checked_add(states[i].remaining)
                .ok_or_else(|| error_time(id, now, OverflowStage::Finish))?;

            if states[i].paused_at.is_some() {
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
            } else {
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
            }

            last_dispatched = Some(id);
            active = Some(Active {
                index: i,
                work_at,
                finish,
                interval_start: now,
            });

            if finish == now {
                active = None;
                let id = states[i].customer.id;
                emit(&mut events, now, id, Kind::Finish);
                completed += 1;
                turnaround_ticks += (now - states[i].customer.arrival) as u128;
                completion_order.push(id);
                continue;
            }

            break;
        }

        // Phase F: expire customers queued or paused during this tick.
        expire_waiters(
            &mut waiting,
            &mut states,
            now,
            &mut events,
            &mut wait_ticks,
            &mut reneged,
        );

        // Jump to the next arrival, active boundary, finish, or waiting deadline.
        let mut next: Option<u64> = None;
        let mut consider = |time: u64| {
            if time > now {
                next = Some(next.map_or(time, |current| current.min(time)));
            }
        };

        if arrival_pos < arrivals.len() {
            consider(states[arrivals[arrival_pos]].customer.arrival);
        }
        if let Some(a) = &active {
            consider(a.finish);
            consider(a.work_at);
        }
        for &i in &waiting {
            if let Some(deadline) = states[i].deadline {
                consider(deadline);
            }
        }

        if let Some(time) = next {
            now = time;
        } else if active.is_none() && waiting.is_empty() && arrival_pos >= arrivals.len() {
            break;
        } else {
            break;
        }
    }

    let elapsed = events.last().map_or(0, |event| event.time);

    let mean_wait_milli = if accepted == 0 {
        0
    } else {
        rounded_ratio(1000 * wait_ticks, accepted as u128).unwrap()
    };
    let mean_turnaround_milli = if completed == 0 {
        0
    } else {
        rounded_ratio(1000 * turnaround_ticks, completed as u128).unwrap()
    };
    let utilization_bps = if elapsed == 0 {
        0
    } else {
        rounded_ratio(10000 * busy_ticks, elapsed as u128).unwrap() as u32
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