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

    Some(if round_up { quotient + 1 } else { quotient })
}

#[derive(Clone)]
struct CustomerState {
    customer: Customer,
    initial_deadline: Option<u64>,
    remaining: u64,
    ever_started: bool,
    queue_entry: u64,
    deadline: Option<u64>,
    remaining_patience: Option<u64>,
    waited: u64,
    pause_time: Option<u64>,
    done: bool,
}

#[derive(Clone, Copy)]
struct Running {
    index: usize,
    work_at: u64,
    finish: u64,
    protected_until: u64,
    segment_start: u64,
}

fn emit(events: &mut Vec<Event>, time: u64, customer: u32, kind: Kind) {
    events.push(Event { time, customer, kind });
}

fn waiting_cmp(a: usize, b: usize, states: &[CustomerState]) -> std::cmp::Ordering {
    let x = &states[a];
    let y = &states[b];

    x.customer
        .priority
        .cmp(&y.customer.priority)
        .then_with(|| match (x.pause_time, y.pause_time) {
            (Some(px), Some(py)) => px.cmp(&py).then_with(|| x.customer.id.cmp(&y.customer.id)),
            (Some(_), None) => std::cmp::Ordering::Less,
            (None, Some(_)) => std::cmp::Ordering::Greater,
            (None, None) => x
                .customer
                .arrival
                .cmp(&y.customer.arrival)
                .then_with(|| y.customer.id.cmp(&x.customer.id)),
        })
}

fn best_waiter(waiting: &[usize], states: &[CustomerState]) -> Option<usize> {
    waiting
        .iter()
        .copied()
        .min_by(|a, b| waiting_cmp(*a, *b, states))
}

fn consume_wait(
    index: usize,
    now: u64,
    states: &mut [CustomerState],
    stats: &mut Stats,
) -> u64 {
    let segment = now - states[index].queue_entry;
    states[index].waited = states[index].waited.saturating_add(segment);
    if let Some(budget) = states[index].remaining_patience {
        states[index].remaining_patience = Some(budget - segment);
    }
    stats.wait_ticks += segment as u128;
    segment
}

fn expire_waiters(
    now: u64,
    waiting: &mut Vec<usize>,
    states: &mut [CustomerState],
    stats: &mut Stats,
    events: &mut Vec<Event>,
) {
    let mut expiring = waiting
        .iter()
        .copied()
        .filter(|i| states[*i].deadline.is_some_and(|d| d <= now))
        .collect::<Vec<_>>();

    expiring.sort_by(|a, b| {
        states[*a]
            .deadline
            .cmp(&states[*b].deadline)
            .then_with(|| states[*a].customer.id.cmp(&states[*b].customer.id))
    });

    for i in expiring {
        if let Some(pos) = waiting.iter().position(|x| *x == i) {
            waiting.remove(pos);
            consume_wait(i, now, states, stats);
            states[i].done = true;
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

    let mut seen = Vec::<u32>::new();
    let mut initial_deadlines = Vec::with_capacity(customers.len());

    for (index, c) in customers.iter().enumerate() {
        if c.id == 0 {
            return Err(Error::ZeroId { index });
        }
        if seen.contains(&c.id) {
            return Err(Error::DuplicateId { index, id: c.id });
        }
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

        seen.push(c.id);
        initial_deadlines.push(deadline);
    }

    let mut states = customers
        .iter()
        .enumerate()
        .map(|(i, c)| CustomerState {
            customer: c.clone(),
            initial_deadline: initial_deadlines[i],
            remaining: c.service,
            ever_started: false,
            queue_entry: c.arrival,
            deadline: initial_deadlines[i],
            remaining_patience: c.patience,
            waited: 0,
            pause_time: None,
            done: false,
        })
        .collect::<Vec<_>>();

    let mut arrivals = (0..customers.len()).collect::<Vec<_>>();
    arrivals.sort_by(|a, b| {
        states[*a]
            .customer
            .arrival
            .cmp(&states[*b].customer.arrival)
            .then_with(|| {
                states[*a]
                    .customer
                    .priority
                    .cmp(&states[*b].customer.priority)
            })
            .then_with(|| states[*b].customer.id.cmp(&states[*a].customer.id))
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
    let mut waiting = Vec::<usize>::new();
    let mut running: Option<Running> = None;
    let mut next_arrival = 0usize;
    let mut last_dispatched: Option<u32> = None;
    let mut now = arrivals
        .first()
        .map(|i| states[*i].customer.arrival)
        .unwrap_or(0);

    while stats.completed + stats.reneged + stats.balked < customers.len() {
        if let Some(r) = running {
            if r.finish == now {
                let id = states[r.index].customer.id;
                stats.busy_ticks += (now - r.segment_start) as u128;
                stats.completed += 1;
                stats.turnaround_ticks +=
                    (now - states[r.index].customer.arrival) as u128;
                stats.completion_order.push(id);
                states[r.index].done = true;
                emit(&mut events, now, id, Kind::Finish);
                running = None;
            }
        }

        expire_waiters(now, &mut waiting, &mut states, &mut stats, &mut events);

        while next_arrival < arrivals.len()
            && states[arrivals[next_arrival]].customer.arrival == now
        {
            let i = arrivals[next_arrival];
            next_arrival += 1;
            let id = states[i].customer.id;
            emit(&mut events, now, id, Kind::Arrive);

            let limit = config.waiting_capacity + usize::from(running.is_none());
            if waiting.len() >= limit {
                stats.balked += 1;
                states[i].done = true;
                emit(&mut events, now, id, Kind::Balk);
            } else {
                stats.accepted += 1;
                states[i].queue_entry = now;
                states[i].deadline = states[i].initial_deadline;
                states[i].remaining_patience = states[i].customer.patience;
                waiting.push(i);
            }
        }

        if let Some(r) = running {
            if now >= r.protected_until {
                if let Some(best) = best_waiter(&waiting, &states) {
                    if states[best].customer.priority < states[r.index].customer.priority {
                        let remaining = r.finish - now;
                        let by = states[best].customer.id;
                        let paused_id = states[r.index].customer.id;

                        emit(
                            &mut events,
                            now,
                            paused_id,
                            Kind::Preempt { remaining, by },
                        );
                        stats.preemptions += 1;
                        stats.busy_ticks += (now - r.segment_start) as u128;

                        states[r.index].remaining = remaining;
                        states[r.index].queue_entry = now;
                        states[r.index].pause_time = Some(now);
                        states[r.index].deadline = match states[r.index].remaining_patience {
                            Some(budget) => Some(now.checked_add(budget).ok_or(
                                Error::TimeOverflow {
                                    customer: paused_id,
                                    time: now,
                                    stage: OverflowStage::Patience,
                                },
                            )?),
                            None => None,
                        };
                        waiting.push(r.index);
                        running = None;
                    }
                }
            }
        }

        loop {
            if running.is_some() {
                break;
            }
            let Some(i) = best_waiter(&waiting, &states) else {
                break;
            };

            waiting.retain(|x| *x != i);
            consume_wait(i, now, &mut states, &mut stats);

            let id = states[i].customer.id;
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
            let finish =
                work_at
                    .checked_add(states[i].remaining)
                    .ok_or(Error::TimeOverflow {
                        customer: id,
                        time: now,
                        stage: OverflowStage::Finish,
                    })?;

            let remaining = states[i].remaining;
            let first = !states[i].ever_started;
            let kind = if first {
                stats.started += 1;
                states[i].ever_started = true;
                Kind::Start {
                    remaining,
                    work_at,
                    finish,
                }
            } else {
                Kind::Resume {
                    remaining,
                    work_at,
                    finish,
                }
            };

            emit(&mut events, now, id, kind);
            last_dispatched = Some(id);
            running = Some(Running {
                index: i,
                work_at,
                finish,
                protected_until: work_at,
                segment_start: now,
            });

            if finish == now {
                stats.completed += 1;
                stats.turnaround_ticks += (now - states[i].customer.arrival) as u128;
                stats.completion_order.push(id);
                states[i].done = true;
                emit(&mut events, now, id, Kind::Finish);
                running = None;
            }
        }

        expire_waiters(now, &mut waiting, &mut states, &mut stats, &mut events);

        if stats.completed + stats.reneged + stats.balked == customers.len() {
            break;
        }

        let mut next = None::<u64>;
        let mut consider = |t: u64| {
            if t > now && next.is_none_or(|old| t < old) {
                next = Some(t);
            }
        };

        if next_arrival < arrivals.len() {
            consider(states[arrivals[next_arrival]].customer.arrival);
        }
        if let Some(r) = running {
            consider(r.finish);
            if r.work_at > now {
                consider(r.work_at);
            }
        }
        for i in &waiting {
            if let Some(d) = states[*i].deadline {
                consider(d);
            }
        }

        if let Some(t) = next {
            now = t;
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
        rounded_ratio(stats.busy_ticks * 10000, stats.elapsed as u128).unwrap() as u32
    };

    Ok(Report { events, stats })
}