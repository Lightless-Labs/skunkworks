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
    Renege {
        waited: u64,
    },
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
    Preempt {
        remaining: u64,
        by: u32,
    },
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
    CapacityTooLarge {
        capacity: usize,
    },
    TooManyCustomers {
        count: usize,
    },
    ZeroId {
        index: usize,
    },
    DuplicateId {
        index: usize,
        id: u32,
    },
    BadPriority {
        index: usize,
        value: u8,
    },
    DeadlineOverflow {
        index: usize,
        id: u32,
    },
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
    remaining: u64,
    started: bool,
    paused: bool,
    pause_time: u64,
    terminal: bool,
}

#[derive(Clone, Copy)]
struct Active {
    index: usize,
    dispatch_time: u64,
    work_at: u64,
    finish: u64,
}

fn emit(events: &mut Vec<Event>, time: u64, customer: u32, kind: Kind) {
    events.push(Event {
        time,
        customer,
        kind,
    });
}

fn waiter_better(a: &CustomerState, b: &CustomerState) -> bool {
    if a.customer.priority != b.customer.priority {
        return a.customer.priority < b.customer.priority;
    }
    if a.paused != b.paused {
        return a.paused;
    }
    if a.paused {
        if a.pause_time != b.pause_time {
            a.pause_time < b.pause_time
        } else {
            a.customer.id < b.customer.id
        }
    } else if a.customer.arrival != b.customer.arrival {
        a.customer.arrival < b.customer.arrival
    } else {
        a.customer.id > b.customer.id
    }
}

fn best_waiter(queue: &[usize], states: &[CustomerState]) -> Option<usize> {
    queue.iter().copied().min_by(|&a, &b| {
        if waiter_better(&states[a], &states[b]) {
            std::cmp::Ordering::Less
        } else if waiter_better(&states[b], &states[a]) {
            std::cmp::Ordering::Greater
        } else {
            std::cmp::Ordering::Equal
        }
    })
}

fn consume_wait(state: &mut CustomerState, time: u64, stats: &mut Stats) {
    let segment = time - state.queue_entry;
    state.waited += segment;
    stats.wait_ticks += segment as u128;

    if let Some(deadline) = state.deadline {
        let remaining_budget = deadline - time;
        state.deadline = Some(time + remaining_budget);
    }
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
        .filter(|&i| states[i].deadline.is_some_and(|deadline| deadline <= now))
        .collect();

    expired.sort_by_key(|&i| (states[i].deadline.unwrap(), states[i].customer.id));

    for i in expired {
        if let Some(position) = queue.iter().position(|&queued| queued == i) {
            queue.remove(position);
        }
        let state = &mut states[i];
        consume_wait(state, now, stats);
        state.terminal = true;
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
        initial_deadlines.push(deadline);
    }

    let mut states: Vec<CustomerState> = customers
        .iter()
        .cloned()
        .enumerate()
        .map(|(index, customer)| CustomerState {
            remaining: customer.service,
            queue_entry: customer.arrival,
            deadline: initial_deadlines[index],
            customer,
            waited: 0,
            started: false,
            paused: false,
            pause_time: 0,
            terminal: false,
        })
        .collect();

    let mut arrivals: Vec<usize> = (0..customers.len()).collect();
    arrivals.sort_by_key(|&index| {
        (
            states[index].customer.arrival,
            states[index].customer.priority,
            std::cmp::Reverse(states[index].customer.id),
        )
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
    let mut next_arrival = 0usize;
    let mut now = 0u64;
    let mut active: Option<Active> = None;
    let mut last_dispatched: Option<u32> = None;
    let mut any_dispatch = false;

    while stats.completed + stats.reneged + stats.balked < customers.len() {
        let next_arrival_time = arrivals
            .get(next_arrival)
            .map(|&i| states[i].customer.arrival);
        let next_finish = active.map(|a| a.finish);
        let next_work = active.map(|a| a.work_at).filter(|&time| time > now);
        let next_deadline = queue
            .iter()
            .filter_map(|&i| states[i].deadline)
            .filter(|&time| time > now)
            .min();

        let mut next_times = Vec::new();
        if let Some(time) = next_arrival_time.filter(|&time| time >= now) {
            next_times.push(time);
        }
        if let Some(time) = next_finish.filter(|&time| time >= now) {
            next_times.push(time);
        }
        if let Some(time) = next_work {
            next_times.push(time);
        }
        if let Some(time) = next_deadline {
            next_times.push(time);
        }

        if next_times.is_empty() {
            break;
        }
        now = *next_times.iter().min().unwrap();

        // Phase A: finish before deadlines, arrivals, or preemption.
        if let Some(current) = active {
            if current.finish == now {
                let id = states[current.index].customer.id;
                stats.busy_ticks += (now - current.dispatch_time) as u128;
                emit(&mut events, now, id, Kind::Finish);
                stats.completed += 1;
                stats.turnaround_ticks +=
                    (now - states[current.index].customer.arrival) as u128;
                stats.completion_order.push(id);
                states[current.index].terminal = true;
                active = None;
            }
        }

        // Phase B: expire those who were waiting at the start of this tick.
        expire_waiters(&mut queue, &mut states, now, &mut events, &mut stats);

        // Phase C: process the complete arrival cohort, without dispatching between arrivals.
        while next_arrival < arrivals.len()
            && states[arrivals[next_arrival]].customer.arrival == now
        {
            let i = arrivals[next_arrival];
            next_arrival += 1;
            let id = states[i].customer.id;
            emit(&mut events, now, id, Kind::Arrive);

            let admission_limit = config.waiting_capacity + usize::from(active.is_none());
            if queue.len() >= admission_limit {
                stats.balked += 1;
                states[i].terminal = true;
                emit(&mut events, now, id, Kind::Balk);
            } else {
                stats.accepted += 1;
                queue.push(i);
            }
        }

        // Phase D: after protection ends, allow one strictly superior waiter to preempt.
        if let Some(current) = active {
            if now >= current.work_at {
                if let Some(waiter) = best_waiter(&queue, &states) {
                    if states[waiter].customer.priority < states[current.index].customer.priority {
                        let remaining = current.finish - now;
                        let paused_id = states[current.index].customer.id;
                        let by = states[waiter].customer.id;
                        emit(
                            &mut events,
                            now,
                            paused_id,
                            Kind::Preempt { remaining, by },
                        );
                        stats.preemptions += 1;
                        stats.busy_ticks += (now - current.dispatch_time) as u128;
                        active = None;

                        let state = &mut states[current.index];
                        state.remaining = remaining;
                        state.queue_entry = now;
                        state.paused = true;
                        state.pause_time = now;

                        if let Some(patience) = state.customer.patience {
                            let remaining_budget = patience - state.waited;
                            state.deadline = Some(now.checked_add(remaining_budget).ok_or(
                                Error::TimeOverflow {
                                    customer: paused_id,
                                    time: now,
                                    stage: OverflowStage::Patience,
                                },
                            )?);
                        } else {
                            state.deadline = None;
                        }
                        queue.push(current.index);
                    }
                }
            }
        }

        // Phase E: dispatch until the server is active or no waiter remains.
        loop {
            if active.is_some() {
                break;
            }
            let Some(i) = best_waiter(&queue, &states) else {
                break;
            };

            let position = queue.iter().position(|&queued| queued == i).unwrap();
            queue.remove(position);

            let id = states[i].customer.id;
            consume_wait(&mut states[i], now, &mut stats);

            let switched = any_dispatch && last_dispatched != Some(id);
            let switch = if switched { config.switch_cost } else { 0 };
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
            if states[i].started {
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
                states[i].started = true;
                stats.started += 1;
            }

            any_dispatch = true;
            last_dispatched = Some(id);
            active = Some(Active {
                index: i,
                dispatch_time: now,
                work_at,
                finish,
            });

            if finish == now {
                emit(&mut events, now, id, Kind::Finish);
                stats.completed += 1;
                stats.turnaround_ticks += (now - states[i].customer.arrival) as u128;
                stats.completion_order.push(id);
                states[i].terminal = true;
                active = None;
                continue;
            }
            break;
        }

        // Phase F: expire any newly queued waiters, including zero-budget arrivals.
        expire_waiters(&mut queue, &mut states, now, &mut events, &mut stats);
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

fn main() {}