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
    let half = denominator / 2;
    let round_up = if denominator % 2 == 0 && r == half {
        q % 2 == 1
    } else {
        r > half
    };
    Some(q + u128::from(round_up))
}

#[derive(Clone)]
struct RuntimeCustomer {
    customer: Customer,
    initial_deadline: Option<u64>,
    accepted: bool,
    terminal: bool,
    started: bool,
    remaining: u64,
    queue_entry: u64,
    waited: u64,
    deadline: Option<u64>,
    paused: bool,
    pause_time: u64,
}

#[derive(Clone, Copy)]
struct Active {
    idx: usize,
    work_at: u64,
    finish: u64,
    dispatch: u64,
}

fn emit(events: &mut Vec<Event>, time: u64, id: u32, kind: Kind) {
    events.push(Event { time, customer: id, kind });
}

fn better(a: usize, b: usize, people: &[RuntimeCustomer]) -> bool {
    let x = &people[a];
    let y = &people[b];
    let xc = &x.customer;
    let yc = &y.customer;

    if xc.priority != yc.priority {
        return xc.priority < yc.priority;
    }
    if x.paused != y.paused {
        return x.paused;
    }
    if x.paused {
        if x.pause_time != y.pause_time {
            return x.pause_time < y.pause_time;
        }
        xc.id < yc.id
    } else if xc.arrival != yc.arrival {
        xc.arrival < yc.arrival
    } else {
        xc.id > yc.id
    }
}

fn best_waiter(waiting: &[usize], people: &[RuntimeCustomer]) -> Option<usize> {
    let mut best = None;
    for &idx in waiting {
        if best.map_or(true, |b| better(idx, b, people)) {
            best = Some(idx);
        }
    }
    best
}

fn expiration_order(waiting: &[usize], people: &[RuntimeCustomer], now: u64) -> Vec<usize> {
    let mut expired: Vec<usize> = waiting
        .iter()
        .copied()
        .filter(|&i| people[i].deadline.is_some_and(|d| d <= now))
        .collect();
    expired.sort_by_key(|&i| (people[i].deadline.unwrap(), people[i].customer.id));
    expired
}

fn consume_wait(idx: usize, now: u64, people: &mut [RuntimeCustomer], stats: &mut Stats) {
    let segment = now - people[idx].queue_entry;
    people[idx].waited += segment;
    stats.wait_ticks += segment as u128;
}

fn renege(
    idx: usize,
    now: u64,
    people: &mut [RuntimeCustomer],
    stats: &mut Stats,
    events: &mut Vec<Event>,
) {
    consume_wait(idx, now, people, stats);
    let id = people[idx].customer.id;
    let waited = people[idx].waited;
    people[idx].terminal = true;
    stats.reneged += 1;
    emit(events, now, id, Kind::Renege { waited });
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
        ids.push(c.id);
        initial_deadlines.push(deadline);
    }

    let mut people: Vec<RuntimeCustomer> = customers
        .iter()
        .cloned()
        .zip(initial_deadlines)
        .map(|(customer, deadline)| RuntimeCustomer {
            remaining: customer.service,
            queue_entry: customer.arrival,
            waited: 0,
            deadline,
            initial_deadline: deadline,
            accepted: false,
            terminal: false,
            started: false,
            paused: false,
            pause_time: 0,
            customer,
        })
        .collect();

    let mut arrivals: Vec<usize> = (0..people.len()).collect();
    arrivals.sort_by_key(|&i| {
        (
            people[i].customer.arrival,
            people[i].customer.priority,
            std::cmp::Reverse(people[i].customer.id),
        )
    });
    let mut arrival_pos = 0usize;
    let mut now = 0u64;
    let mut active: Option<Active> = None;
    let mut waiting: Vec<usize> = Vec::new();
    let mut last_dispatched: Option<u32> = None;
    let mut first_dispatch = true;
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

    while arrival_pos < arrivals.len() || active.is_some() || !waiting.is_empty() {
        let mut next = None;
        if let Some(&i) = arrivals.get(arrival_pos) {
            next = Some(people[i].customer.arrival);
        }
        if let Some(a) = active {
            for t in [Some(a.finish), (a.work_at > now).then_some(a.work_at)]
                .into_iter()
                .flatten()
            {
                next = Some(next.map_or(t, |n| n.min(t)));
            }
        }
        for &i in &waiting {
            if let Some(d) = people[i].deadline {
                if d > now {
                    next = Some(next.map_or(d, |n| n.min(d)));
                } else {
                    next = Some(now);
                }
            }
        }
        let Some(t) = next else { break };
        now = t;

        if let Some(a) = active {
            if a.finish == now {
                let idx = a.idx;
                let id = people[idx].customer.id;
                stats.busy_ticks += (now - a.dispatch) as u128;
                stats.completed += 1;
                stats.turnaround_ticks += (now - people[idx].customer.arrival) as u128;
                stats.completion_order.push(id);
                people[idx].terminal = true;
                active = None;
                emit(&mut events, now, id, Kind::Finish);
            }
        }

        let expired = expiration_order(&waiting, &people, now);
        for idx in expired {
            waiting.retain(|&x| x != idx);
            renege(idx, now, &mut people, &mut stats, &mut events);
        }

        while let Some(&idx) = arrivals.get(arrival_pos) {
            if people[idx].customer.arrival != now {
                break;
            }
            arrival_pos += 1;
            let id = people[idx].customer.id;
            emit(&mut events, now, id, Kind::Arrive);
            let limit = config.waiting_capacity + usize::from(active.is_none());
            if waiting.len() >= limit {
                stats.balked += 1;
                people[idx].terminal = true;
                emit(&mut events, now, id, Kind::Balk);
            } else {
                people[idx].accepted = true;
                stats.accepted += 1;
                people[idx].queue_entry = now;
                people[idx].deadline = people[idx].initial_deadline;
                waiting.push(idx);
            }
        }

        if let Some(a) = active {
            if now >= a.work_at {
                if let Some(best) = best_waiter(&waiting, &people) {
                    if people[best].customer.priority < people[a.idx].customer.priority {
                        let paused = a.idx;
                        let remaining = a.finish - now;
                        let by = people[best].customer.id;
                        let id = people[paused].customer.id;
                        stats.busy_ticks += (now - a.dispatch) as u128;
                        emit(&mut events, now, id, Kind::Preempt { remaining, by });
                        stats.preemptions += 1;
                        active = None;
                        people[paused].remaining = remaining;
                        people[paused].paused = true;
                        people[paused].pause_time = now;
                        people[paused].queue_entry = now;
                        people[paused].deadline = match people[paused].customer.patience {
                            Some(_) => Some(
                                now.checked_add(
                                    people[paused].customer.patience.unwrap()
                                        - people[paused].waited,
                                )
                                .ok_or(Error::TimeOverflow {
                                    customer: id,
                                    time: now,
                                    stage: OverflowStage::Patience,
                                })?,
                            ),
                            None => None,
                        };
                        waiting.push(paused);
                    }
                }
            }
        }

        loop {
            if active.is_some() {
                break;
            }
            let Some(idx) = best_waiter(&waiting, &people) else {
                break;
            };
            waiting.retain(|&x| x != idx);
            consume_wait(idx, now, &mut people, &mut stats);
            let id = people[idx].customer.id;
            let cost = if first_dispatch || last_dispatched == Some(id) {
                0
            } else {
                config.switch_cost
            };
            first_dispatch = false;
            last_dispatched = Some(id);
            let work_at = now.checked_add(cost).ok_or(Error::TimeOverflow {
                customer: id,
                time: now,
                stage: OverflowStage::WorkAt,
            })?;
            let finish = work_at
                .checked_add(people[idx].remaining)
                .ok_or(Error::TimeOverflow {
                    customer: id,
                    time: now,
                    stage: OverflowStage::Finish,
                })?;
            let remaining = people[idx].remaining;
            let was_started = people[idx].started;
            if !was_started {
                people[idx].started = true;
                stats.started += 1;
            }
            emit(
                &mut events,
                now,
                id,
                if was_started {
                    Kind::Resume {
                        remaining,
                        work_at,
                        finish,
                    }
                } else {
                    Kind::Start {
                        remaining,
                        work_at,
                        finish,
                    }
                },
            );
            active = Some(Active {
                idx,
                work_at,
                finish,
                dispatch: now,
            });
            if finish == now {
                stats.completed += 1;
                stats.turnaround_ticks += (now - people[idx].customer.arrival) as u128;
                stats.completion_order.push(id);
                people[idx].terminal = true;
                active = None;
                emit(&mut events, now, id, Kind::Finish);
                continue;
            }
        }

        let expired = expiration_order(&waiting, &people, now);
        for idx in expired {
            waiting.retain(|&x| x != idx);
            renege(idx, now, &mut people, &mut stats, &mut events);
        }
    }

    stats.elapsed = events.last().map_or(0, |e| e.time);
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
        rounded_ratio(stats.busy_ticks * 10000, stats.elapsed as u128).unwrap() as u32
    };
    Ok(Report { events, stats })
}