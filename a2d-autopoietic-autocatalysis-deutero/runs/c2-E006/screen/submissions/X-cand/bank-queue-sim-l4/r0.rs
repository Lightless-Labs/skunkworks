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
struct Waiting {
    index: usize,
    entry: u64,
    deadline: Option<u64>,
    remaining_budget: Option<u64>,
    waited: u64,
    paused: bool,
    pause_time: u64,
    remaining: u64,
}

#[derive(Clone)]
struct Running {
    index: usize,
    work_at: u64,
    finish: u64,
    remaining_budget: Option<u64>,
    waited: u64,
}

fn waiting_precedes(a: &Waiting, b: &Waiting, customers: &[Customer]) -> bool {
    let ca = &customers[a.index];
    let cb = &customers[b.index];

    if ca.priority != cb.priority {
        return ca.priority < cb.priority;
    }
    if a.paused != b.paused {
        return a.paused;
    }
    if a.paused {
        if a.pause_time != b.pause_time {
            return a.pause_time < b.pause_time;
        }
        ca.id < cb.id
    } else if ca.arrival != cb.arrival {
        ca.arrival < cb.arrival
    } else {
        ca.id > cb.id
    }
}

fn best_waiter(waiting: &[Waiting], customers: &[Customer]) -> Option<usize> {
    let mut best = None;
    for i in 0..waiting.len() {
        if best.map_or(true, |j| waiting_precedes(&waiting[i], &waiting[j], customers)) {
            best = Some(i);
        }
    }
    best
}

fn emit(events: &mut Vec<Event>, time: u64, customer: u32, kind: Kind) {
    events.push(Event { time, customer, kind });
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

    for (i, c) in customers.iter().enumerate() {
        if c.id == 0 {
            return Err(Error::ZeroId { index: i });
        }
        if customers[..i].iter().any(|previous| previous.id == c.id) {
            return Err(Error::DuplicateId { index: i, id: c.id });
        }
        if c.priority > 3 {
            return Err(Error::BadPriority {
                index: i,
                value: c.priority,
            });
        }
        if let Some(patience) = c.patience {
            if c.arrival.checked_add(patience).is_none() {
                return Err(Error::DeadlineOverflow { index: i, id: c.id });
            }
        }
    }

    let mut arrivals: Vec<usize> = (0..customers.len()).collect();
    arrivals.sort_by(|&a, &b| {
        customers[a]
            .arrival
            .cmp(&customers[b].arrival)
            .then_with(|| customers[a].priority.cmp(&customers[b].priority))
            .then_with(|| customers[b].id.cmp(&customers[a].id))
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

    if customers.is_empty() {
        return Ok(Report { events, stats });
    }

    let mut waiting: Vec<Waiting> = Vec::new();
    let mut running: Option<Running> = None;
    let mut next_arrival = 0usize;
    let mut now = customers[arrivals[0]].arrival;
    let mut ever_started = vec![false; customers.len()];
    let mut last_dispatched: Option<u32> = None;
    let mut first_dispatch = true;

    loop {
        // Phase A: completion wins over all other activity at this tick.
        if running.as_ref().is_some_and(|r| r.finish == now) {
            let r = running.take().unwrap();
            let c = &customers[r.index];
            emit(&mut events, now, c.id, Kind::Finish);
            stats.completed += 1;
            stats.turnaround_ticks += (now - c.arrival) as u128;
            stats.completion_order.push(c.id);
        }

        // Phase B: expire customers that were already waiting.
        let mut expiring: Vec<usize> = waiting
            .iter()
            .enumerate()
            .filter_map(|(i, w)| w.deadline.filter(|&d| d <= now).map(|d| (i, d)))
            .map(|(i, _)| i)
            .collect();
        expiring.sort_by(|&a, &b| {
            waiting[a]
                .deadline
                .cmp(&waiting[b].deadline)
                .then_with(|| {
                    customers[waiting[a].index]
                        .id
                        .cmp(&customers[waiting[b].index].id)
                })
        });
        for i in expiring.into_iter().rev() {
            let w = waiting.remove(i);
            let segment = now - w.entry;
            let total = w.waited + segment;
            stats.wait_ticks += segment as u128;
            emit(
                &mut events,
                now,
                customers[w.index].id,
                Kind::Renege { waited: total },
            );
            stats.reneged += 1;
        }

        // Phase C: process the entire arrival cohort against the same gate.
        while next_arrival < arrivals.len()
            && customers[arrivals[next_arrival]].arrival == now
        {
            let idx = arrivals[next_arrival];
            next_arrival += 1;
            let c = &customers[idx];
            emit(&mut events, now, c.id, Kind::Arrive);

            let limit = config.waiting_capacity + usize::from(running.is_none());
            if waiting.len() >= limit {
                emit(&mut events, now, c.id, Kind::Balk);
                stats.balked += 1;
            } else {
                waiting.push(Waiting {
                    index: idx,
                    entry: now,
                    deadline: c.patience.map(|p| c.arrival + p),
                    remaining_budget: c.patience,
                    waited: 0,
                    paused: false,
                    pause_time: 0,
                    remaining: c.service,
                });
                stats.accepted += 1;
            }
        }

        // Phase D: at most one preemption, after switching protection ends.
        if let Some(r) = running.as_ref() {
            if now >= r.work_at {
                if let Some(bi) = best_waiter(&waiting, customers) {
                    if customers[waiting[bi].index].priority < customers[r.index].priority {
                        let selected_id = customers[waiting[bi].index].id;
                        let mut r = running.take().unwrap();
                        let remaining = r.finish - now;

                        emit(
                            &mut events,
                            now,
                            customers[r.index].id,
                            Kind::Preempt {
                                remaining,
                                by: selected_id,
                            },
                        );
                        stats.preemptions += 1;

                        let deadline = match r.remaining_budget {
                            Some(budget) => Some(now.checked_add(budget).ok_or(
                                Error::TimeOverflow {
                                    customer: customers[r.index].id,
                                    time: now,
                                    stage: OverflowStage::Patience,
                                },
                            )?),
                            None => None,
                        };

                        waiting.push(Waiting {
                            index: r.index,
                            entry: now,
                            deadline,
                            remaining_budget: r.remaining_budget,
                            waited: r.waited,
                            paused: true,
                            pause_time: now,
                            remaining,
                        });
                    }
                }
            }
        }

        // Phase E: dispatch repeatedly through any inline completions.
        loop {
            if running.is_some() {
                break;
            }
            let Some(wi) = best_waiter(&waiting, customers) else {
                break;
            };
            let mut w = waiting.remove(wi);
            let segment = now - w.entry;
            w.waited += segment;
            stats.wait_ticks += segment as u128;

            if let Some(budget) = w.remaining_budget.as_mut() {
                *budget -= segment;
            }

            let c = &customers[w.index];
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

            let kind = if ever_started[w.index] {
                Kind::Resume {
                    remaining: w.remaining,
                    work_at,
                    finish,
                }
            } else {
                ever_started[w.index] = true;
                stats.started += 1;
                Kind::Start {
                    remaining: w.remaining,
                    work_at,
                    finish,
                }
            };
            emit(&mut events, now, c.id, kind);
            first_dispatch = false;
            last_dispatched = Some(c.id);

            if finish == now {
                emit(&mut events, now, c.id, Kind::Finish);
                stats.completed += 1;
                stats.turnaround_ticks += (now - c.arrival) as u128;
                stats.completion_order.push(c.id);
                continue;
            }

            running = Some(Running {
                index: w.index,
                work_at,
                finish,
                remaining_budget: w.remaining_budget,
                waited: w.waited,
            });
            break;
        }

        // Phase F: expire those still waiting after dispatch opportunities.
        let mut expiring: Vec<usize> = waiting
            .iter()
            .enumerate()
            .filter_map(|(i, w)| w.deadline.filter(|&d| d <= now).map(|d| (i, d)))
            .map(|(i, _)| i)
            .collect();
        expiring.sort_by(|&a, &b| {
            waiting[a]
                .deadline
                .cmp(&waiting[b].deadline)
                .then_with(|| {
                    customers[waiting[a].index]
                        .id
                        .cmp(&customers[waiting[b].index].id)
                })
        });
        for i in expiring.into_iter().rev() {
            let w = waiting.remove(i);
            let segment = now - w.entry;
            let total = w.waited + segment;
            stats.wait_ticks += segment as u128;
            emit(
                &mut events,
                now,
                customers[w.index].id,
                Kind::Renege { waited: total },
            );
            stats.reneged += 1;
        }

        if next_arrival == arrivals.len() && running.is_none() && waiting.is_empty() {
            break;
        }

        let mut next_time: Option<u64> = None;
        let mut consider = |t: u64| {
            if t > now && next_time.map_or(true, |current| t < current) {
                next_time = Some(t);
            }
        };

        if next_arrival < arrivals.len() {
            consider(customers[arrivals[next_arrival]].arrival);
        }
        if let Some(r) = &running {
            consider(r.finish);
            if r.work_at > now {
                consider(r.work_at);
            }
        }
        for w in &waiting {
            if let Some(deadline) = w.deadline {
                consider(deadline);
            }
        }

        if let Some(next) = next_time {
            if running.is_some() {
                stats.busy_ticks += (next - now) as u128;
            }
            now = next;
        } else {
            break;
        }
    }

    stats.elapsed = events.last().map_or(0, |event| event.time);
    stats.mean_wait_milli = if stats.accepted == 0 {
        0
    } else {
        rounded_ratio(1000 * stats.wait_ticks, stats.accepted as u128).unwrap()
    };
    stats.mean_turnaround_milli = if stats.completed == 0 {
        0
    } else {
        rounded_ratio(1000 * stats.turnaround_ticks, stats.completed as u128).unwrap()
    };
    stats.utilization_bps = if stats.elapsed == 0 {
        0
    } else {
        rounded_ratio(10000 * stats.busy_ticks, stats.elapsed as u128)
            .unwrap()
            .min(u32::MAX as u128) as u32
    };

    Ok(Report { events, stats })
}