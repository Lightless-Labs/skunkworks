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
    let round_up = remainder > half
        && (denominator % 2 == 0 || remainder != half + 1)
        || (denominator % 2 == 0
            && remainder == half
            && quotient % 2 == 1);
    Some(if round_up { quotient + 1 } else { quotient })
}

#[derive(Clone)]
struct Waiter {
    index: usize,
    entry: u64,
    deadline: Option<u64>,
    waited: u64,
    remaining: u64,
    paused: bool,
    pause_time: u64,
}

struct Active {
    index: usize,
    remaining: u64,
    work_at: u64,
    finish: u64,
    dispatched_at: u64,
}

fn emit(events: &mut Vec<Event>, time: u64, id: u32, kind: Kind) {
    events.push(Event { time, customer: id, kind });
}

fn better(a: &Waiter, b: &Waiter, customers: &[Customer]) -> bool {
    let ca = &customers[a.index];
    let cb = &customers[b.index];

    if ca.priority != cb.priority {
        return ca.priority < cb.priority;
    }
    if a.paused != b.paused {
        return a.paused;
    }
    if a.paused {
        (a.pause_time, ca.id) < (b.pause_time, cb.id)
    } else {
        (ca.arrival, std::cmp::Reverse(ca.id))
            < (cb.arrival, std::cmp::Reverse(cb.id))
    }
}

fn best_waiter(waiting: &[Waiter], customers: &[Customer]) -> Option<usize> {
    let mut best = None;
    for i in 0..waiting.len() {
        if best.map_or(true, |b| better(&waiting[i], &waiting[b], customers)) {
            best = Some(i);
        }
    }
    best
}

fn expire_waiters(
    waiting: &mut Vec<Waiter>,
    customers: &[Customer],
    now: u64,
    events: &mut Vec<Event>,
    wait_ticks: &mut u128,
    reneged: &mut usize,
) {
    let mut expired: Vec<usize> = waiting
        .iter()
        .enumerate()
        .filter_map(|(i, w)| w.deadline.filter(|&d| d <= now).map(|d| (d, customers[w.index].id, i)))
        .map(|(_, _, i)| i)
        .collect();

    expired.sort_by(|&a, &b| {
        waiting[a]
            .deadline
            .cmp(&waiting[b].deadline)
            .then_with(|| customers[waiting[a].index].id.cmp(&customers[waiting[b].index].id))
    });

    for i in expired.into_iter().rev() {
        let w = waiting.remove(i);
        let segment = now - w.entry;
        *wait_ticks += segment as u128;
        *reneged += 1;
        emit(
            events,
            now,
            customers[w.index].id,
            Kind::Renege {
                waited: w.waited + segment,
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

    for (index, c) in customers.iter().enumerate() {
        if c.id == 0 {
            return Err(Error::ZeroId { index });
        }
        if customers[..index].iter().any(|previous| previous.id == c.id) {
            return Err(Error::DuplicateId { index, id: c.id });
        }
        if c.priority > 3 {
            return Err(Error::BadPriority {
                index,
                value: c.priority,
            });
        }
        if c.patience.is_some_and(|p| c.arrival.checked_add(p).is_none()) {
            return Err(Error::DeadlineOverflow { index, id: c.id });
        }
    }

    let mut order: Vec<usize> = (0..customers.len()).collect();
    order.sort_by(|&a, &b| {
        customers[a]
            .arrival
            .cmp(&customers[b].arrival)
            .then_with(|| customers[a].priority.cmp(&customers[b].priority))
            .then_with(|| customers[b].id.cmp(&customers[a].id))
    });

    let mut events = Vec::new();
    let mut waiting: Vec<Waiter> = Vec::new();
    let mut active: Option<Active> = None;
    let mut cursor = 0usize;
    let mut now = order.first().map_or(0, |&i| customers[i].arrival);
    let mut last_dispatched = None;
    let mut first_dispatch = true;
    let mut started_once = vec![false; customers.len()];

    let (mut accepted, mut balked, mut reneged, mut completed, mut started, mut preemptions) =
        (0usize, 0usize, 0usize, 0usize, 0usize, 0usize);
    let (mut busy_ticks, mut wait_ticks, mut turnaround_ticks) = (0u128, 0u128, 0u128);
    let mut completion_order = Vec::new();

    while cursor < order.len() || active.is_some() || !waiting.is_empty() {
        // Phase A: finish active work before any other action at this tick.
        if active.as_ref().is_some_and(|a| a.finish == now) {
            let done = active.take().unwrap();
            let c = &customers[done.index];
            busy_ticks += (now - done.dispatched_at) as u128;
            emit(&mut events, now, c.id, Kind::Finish);
            completed += 1;
            turnaround_ticks += (now - c.arrival) as u128;
            completion_order.push(c.id);
        }

        // Phase B: expire waiters that were already queued.
        expire_waiters(
            &mut waiting,
            customers,
            now,
            &mut events,
            &mut wait_ticks,
            &mut reneged,
        );

        // Phase C: admit the entire arrival cohort without dispatching between arrivals.
        while cursor < order.len() && customers[order[cursor]].arrival == now {
            let index = order[cursor];
            cursor += 1;
            let c = &customers[index];
            emit(&mut events, now, c.id, Kind::Arrive);

            let gate = config.waiting_capacity + usize::from(active.is_none());
            if waiting.len() >= gate {
                emit(&mut events, now, c.id, Kind::Balk);
                balked += 1;
            } else {
                accepted += 1;
                waiting.push(Waiter {
                    index,
                    entry: now,
                    deadline: c.patience.map(|p| c.arrival + p),
                    waited: 0,
                    remaining: c.service,
                    paused: false,
                    pause_time: 0,
                });
            }
        }

        // Phase D: once switching protection ends, compare against the best waiter.
        if active.as_ref().is_some_and(|a| now >= a.work_at) {
            if let Some(wi) = best_waiter(&waiting, customers) {
                let running_index = active.as_ref().unwrap().index;
                if customers[waiting[wi].index].priority < customers[running_index].priority {
                    let by = customers[waiting[wi].index].id;
                    let old = active.take().unwrap();
                    let c = &customers[old.index];
                    let remaining = old.finish - now;
                    busy_ticks += (now - old.dispatched_at) as u128;
                    emit(&mut events, now, c.id, Kind::Preempt { remaining, by });
                    preemptions += 1;

                    let waited_total = waiting
                        .iter()
                        .find(|w| w.index == old.index)
                        .map_or(0, |w| w.waited);

                    let already_waited = c.patience.map_or(0, |budget| {
                        budget.saturating_sub(
                            waiting
                                .iter()
                                .find(|w| w.index == old.index)
                                .and_then(|w| w.deadline)
                                .map_or(0, |d| d.saturating_sub(waiting.iter().find(|w| w.index == old.index).map_or(now, |w| w.entry))),
                        )
                    });
                    let cumulative_waited = waited_total.max(already_waited);

                    let remaining_budget = c.patience.map(|budget| budget.saturating_sub(cumulative_waited));
                    let deadline = match remaining_budget {
                        Some(budget) => Some(now.checked_add(budget).ok_or(Error::TimeOverflow {
                            customer: c.id,
                            time: now,
                            stage: OverflowStage::Patience,
                        })?),
                        None => None,
                    };

                    waiting.push(Waiter {
                        index: old.index,
                        entry: now,
                        deadline,
                        waited: cumulative_waited,
                        remaining,
                        paused: true,
                        pause_time: now,
                    });
                }
            }
        }

        // Phase E: dispatch while idle, including inline zero-duration finishes.
        loop {
            if active.is_some() {
                break;
            }
            let Some(wi) = best_waiter(&waiting, customers) else {
                break;
            };
            let w = waiting.remove(wi);
            let c = &customers[w.index];
            let segment = now - w.entry;
            let total_waited = w.waited + segment;
            wait_ticks += segment as u128;

            let switch = if first_dispatch || last_dispatched == Some(c.id) {
                0
            } else {
                config.switch_cost
            };
            let work_at = now.checked_add(switch).ok_or(Error::TimeOverflow {
                customer: c.id,
                time: now,
                stage: OverflowStage::WorkAt,
            })?;
            let finish = work_at.checked_add(w.remaining).ok_or(Error::TimeOverflow {
                customer: c.id,
                time: now,
                stage: OverflowStage::Finish,
            })?;

            let kind = if started_once[w.index] {
                Kind::Resume {
                    remaining: w.remaining,
                    work_at,
                    finish,
                }
            } else {
                started_once[w.index] = true;
                started += 1;
                Kind::Start {
                    remaining: w.remaining,
                    work_at,
                    finish,
                }
            };
            emit(&mut events, now, c.id, kind);
            last_dispatched = Some(c.id);
            first_dispatch = false;
            active = Some(Active {
                index: w.index,
                remaining: w.remaining,
                work_at,
                finish,
                dispatched_at: now,
            });

            if finish == now {
                active = None;
                emit(&mut events, now, c.id, Kind::Finish);
                completed += 1;
                turnaround_ticks += (now - c.arrival) as u128;
                completion_order.push(c.id);
                continue;
            }
            break;
        }

        // Phase F: expire any waiters queued during phases C or D.
        expire_waiters(
            &mut waiting,
            customers,
            now,
            &mut events,
            &mut wait_ticks,
            &mut reneged,
        );

        if cursor >= order.len() && active.is_none() && waiting.is_empty() {
            break;
        }

        let mut next: Option<u64> = None;
        if cursor < order.len() {
            next = Some(customers[order[cursor]].arrival);
        }
        if let Some(a) = &active {
            for t in [a.work_at, a.finish] {
                if t > now {
                    next = Some(next.map_or(t, |n| n.min(t)));
                }
            }
        }
        for w in &waiting {
            if let Some(d) = w.deadline {
                if d > now {
                    next = Some(next.map_or(d, |n| n.min(d)));
                }
            }
        }

        if let Some(t) = next {
            now = t;
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