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
    let up = if denominator % 2 == 0 {
        r > half || (r == half && q % 2 == 1)
    } else {
        r > half
    };
    Some(if up { q + 1 } else { q })
}

#[derive(Clone)]
struct Person {
    customer: Customer,
    initial_deadline: Option<u64>,
}

#[derive(Clone)]
struct Waiter {
    index: usize,
    entry: u64,
    deadline: Option<u64>,
    waited: u64,
    paused: bool,
    pause_time: u64,
    remaining: u64,
}

#[derive(Clone)]
struct Active {
    index: usize,
    remaining: u64,
    dispatch: u64,
    work_at: u64,
    finish: u64,
}

fn waiter_better(a: &Waiter, b: &Waiter, people: &[Person]) -> bool {
    let ca = &people[a.index].customer;
    let cb = &people[b.index].customer;
    if ca.priority != cb.priority {
        return ca.priority < cb.priority;
    }
    if a.paused != b.paused {
        return a.paused;
    }
    if a.paused {
        a.pause_time < b.pause_time
            || (a.pause_time == b.pause_time && ca.id < cb.id)
    } else {
        ca.arrival < cb.arrival || (ca.arrival == cb.arrival && ca.id > cb.id)
    }
}

fn best_waiter(waiting: &[Waiter], people: &[Person]) -> Option<usize> {
    let mut best = None;
    for i in 0..waiting.len() {
        if best.map_or(true, |j| waiter_better(&waiting[i], &waiting[j], people)) {
            best = Some(i);
        }
    }
    best
}

fn overflow(customer: u32, time: u64, stage: OverflowStage) -> Error {
    Error::TimeOverflow { customer, time, stage }
}

fn expire(
    waiting: &mut Vec<Waiter>,
    people: &[Person],
    now: u64,
    events: &mut Vec<Event>,
    stats: &mut Stats,
) {
    let mut expired = Vec::new();
    let mut keep = Vec::with_capacity(waiting.len());
    for w in waiting.drain(..) {
        if w.deadline.is_some_and(|d| d <= now) {
            expired.push(w);
        } else {
            keep.push(w);
        }
    }
    expired.sort_by(|a, b| {
        a.deadline
            .cmp(&b.deadline)
            .then(people[a.index].customer.id.cmp(&people[b.index].customer.id))
    });
    *waiting = keep;

    for w in expired {
        let segment = now - w.entry;
        stats.wait_ticks += segment as u128;
        let waited = w.waited + segment;
        let id = people[w.index].customer.id;
        events.push(Event {
            time: now,
            customer: id,
            kind: Kind::Renege { waited },
        });
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

    let mut people = Vec::with_capacity(customers.len());
    for (index, c) in customers.iter().enumerate() {
        if c.id == 0 {
            return Err(Error::ZeroId { index });
        }
        if customers[..index].iter().any(|x| x.id == c.id) {
            return Err(Error::DuplicateId { index, id: c.id });
        }
        if c.priority > 3 {
            return Err(Error::BadPriority {
                index,
                value: c.priority,
            });
        }
        let initial_deadline = match c.patience {
            Some(p) => Some(
                c.arrival
                    .checked_add(p)
                    .ok_or(Error::DeadlineOverflow { index, id: c.id })?,
            ),
            None => None,
        };
        people.push(Person {
            customer: c.clone(),
            initial_deadline,
        });
    }

    let mut arrivals: Vec<usize> = (0..people.len()).collect();
    arrivals.sort_by(|&a, &b| {
        let ca = &people[a].customer;
        let cb = &people[b].customer;
        ca.arrival
            .cmp(&cb.arrival)
            .then(ca.priority.cmp(&cb.priority))
            .then(cb.id.cmp(&ca.id))
    });

    let mut events = Vec::new();
    let mut waiting: Vec<Waiter> = Vec::new();
    let mut active: Option<Active> = None;
    let mut arrival_pos = 0usize;
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

    while stats.completed + stats.reneged + stats.balked < people.len() {
        let mut next = arrivals
            .get(arrival_pos)
            .map(|&i| people[i].customer.arrival);

        if let Some(a) = &active {
            next = Some(next.map_or(a.finish, |v| v.min(a.finish)));
            if a.work_at > now {
                next = Some(next.map_or(a.work_at, |v| v.min(a.work_at)));
            }
        }
        for w in &waiting {
            if let Some(d) = w.deadline {
                next = Some(next.map_or(d, |v| v.min(d)));
            }
        }

        let Some(t) = next else { break };
        now = t;

        // Phase A: finish before any other work at this time.
        if active.as_ref().is_some_and(|a| a.finish == now) {
            let a = active.take().unwrap();
            let c = &people[a.index].customer;
            stats.busy_ticks += (now - a.dispatch) as u128;
            events.push(Event {
                time: now,
                customer: c.id,
                kind: Kind::Finish,
            });
            stats.completed += 1;
            stats.turnaround_ticks += (now - c.arrival) as u128;
            stats.completion_order.push(c.id);
        }

        // Phase B.
        expire(&mut waiting, &people, now, &mut events, &mut stats);

        // Phase C.
        while let Some(&i) = arrivals.get(arrival_pos) {
            if people[i].customer.arrival != now {
                break;
            }
            arrival_pos += 1;
            let c = &people[i].customer;
            events.push(Event {
                time: now,
                customer: c.id,
                kind: Kind::Arrive,
            });
            let limit = config.waiting_capacity + usize::from(active.is_none());
            if waiting.len() >= limit {
                events.push(Event {
                    time: now,
                    customer: c.id,
                    kind: Kind::Balk,
                });
                stats.balked += 1;
            } else {
                stats.accepted += 1;
                waiting.push(Waiter {
                    index: i,
                    entry: now,
                    deadline: people[i].initial_deadline,
                    waited: 0,
                    paused: false,
                    pause_time: 0,
                    remaining: c.service,
                });
            }
        }

        // Phase D.
        if let Some(a) = active.as_ref() {
            if now >= a.work_at {
                if let Some(wi) = best_waiter(&waiting, &people) {
                    if people[waiting[wi].index].customer.priority
                        < people[a.index].customer.priority
                    {
                        let paused = active.take().unwrap();
                        let waiter = waiting.remove(wi);
                        let paused_id = people[paused.index].customer.id;
                        let by = people[waiter.index].customer.id;
                        let remaining = paused.finish - now;
                        stats.busy_ticks += (now - paused.dispatch) as u128;
                        events.push(Event {
                            time: now,
                            customer: paused_id,
                            kind: Kind::Preempt { remaining, by },
                        });

                        let deadline = match people[paused.index].customer.patience {
                            Some(budget) => Some(now.checked_add(budget).ok_or_else(|| {
                                overflow(paused_id, now, OverflowStage::Patience)
                            })?),
                            None => None,
                        };
                        waiting.push(Waiter {
                            index: paused.index,
                            entry: now,
                            deadline,
                            waited: 0,
                            paused: true,
                            pause_time: now,
                            remaining,
                        });
                        waiting.push(waiter);
                        // The paused customer has been placed back in the queue;
                        // dispatch is handled below in Phase E.
                    }
                }
            }
        }

        // Phase E: dispatch until occupied or no waiter remains. Inline finishes
        // continue within this phase.
        loop {
            if active.is_some() {
                break;
            }
            let Some(wi) = best_waiter(&waiting, &people) else {
                break;
            };
            let w = waiting.remove(wi);
            let c = &people[w.index].customer;
            let segment = now - w.entry;
            let total_waited = w.waited + segment;
            stats.wait_ticks += segment as u128;

            let cost = if last_dispatch.is_some_and(|id| id != c.id) {
                config.switch_cost
            } else {
                0
            };
            let work_at = now
                .checked_add(cost)
                .ok_or_else(|| overflow(c.id, now, OverflowStage::WorkAt))?;
            let finish = work_at
                .checked_add(w.remaining)
                .ok_or_else(|| overflow(c.id, now, OverflowStage::Finish))?;

            let is_first = !w.paused;
            if is_first {
                stats.started += 1;
            }
            events.push(Event {
                time: now,
                customer: c.id,
                kind: if is_first {
                    Kind::Start {
                        remaining: w.remaining,
                        work_at,
                        finish,
                    }
                } else {
                    Kind::Resume {
                        remaining: w.remaining,
                        work_at,
                        finish,
                    }
                },
            });
            last_dispatch = Some(c.id);
            active = Some(Active {
                index: w.index,
                remaining: w.remaining,
                dispatch: now,
                work_at,
                finish,
            });

            if finish == now {
                let a = active.take().unwrap();
                stats.busy_ticks += (now - a.dispatch) as u128;
                events.push(Event {
                    time: now,
                    customer: c.id,
                    kind: Kind::Finish,
                });
                stats.completed += 1;
                stats.turnaround_ticks += (now - c.arrival) as u128;
                stats.completion_order.push(c.id);
                continue;
            }
        }

        // Phase F.
        expire(&mut waiting, &people, now, &mut events, &mut stats);
    }

    stats.elapsed = events.last().map_or(0, |e| e.time);
    if stats.accepted != 0 {
        stats.mean_wait_milli =
            rounded_ratio(stats.wait_ticks * 1000, stats.accepted as u128).unwrap_or(0);
    }
    if stats.completed != 0 {
        stats.mean_turnaround_milli = rounded_ratio(
            stats.turnaround_ticks * 1000,
            stats.completed as u128,
        )
        .unwrap_or(0);
    }
    if stats.elapsed != 0 {
        stats.utilization_bps =
            rounded_ratio(stats.busy_ticks * 10000, stats.elapsed as u128).unwrap_or(0) as u32;
    }

    Ok(Report { events, stats })
}

fn main() {}