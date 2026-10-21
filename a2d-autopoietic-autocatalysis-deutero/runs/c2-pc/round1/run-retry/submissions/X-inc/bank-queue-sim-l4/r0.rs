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
struct Job {
    customer: Customer,
    deadline: Option<u64>,
    remaining: u64,
    queue_since: u64,
    waited: u64,
    spent_wait: u64,
    pause_time: Option<u64>,
    ever_started: bool,
}

#[derive(Clone)]
struct Active {
    job: usize,
    work_at: u64,
    finish: u64,
    dispatch_time: u64,
}

fn emit(events: &mut Vec<Event>, time: u64, customer: u32, kind: Kind) {
    events.push(Event { time, customer, kind });
}

fn better_waiter(a: &Job, b: &Job) -> bool {
    if a.customer.priority != b.customer.priority {
        return a.customer.priority < b.customer.priority;
    }

    match (a.pause_time, b.pause_time) {
        (Some(pa), Some(pb)) => {
            if pa != pb {
                pa < pb
            } else {
                a.customer.id < b.customer.id
            }
        }
        (Some(_), None) => true,
        (None, Some(_)) => false,
        (None, None) => {
            if a.customer.arrival != b.customer.arrival {
                a.customer.arrival < b.customer.arrival
            } else {
                a.customer.id > b.customer.id
            }
        }
    }
}

fn best_waiter(waiting: &[usize], jobs: &[Job]) -> Option<usize> {
    let mut best = None;
    for &idx in waiting {
        if best.map_or(true, |old| better_waiter(&jobs[idx], &jobs[old])) {
            best = Some(idx);
        }
    }
    best
}

fn remove_waiter(waiting: &mut Vec<usize>, idx: usize) {
    if let Some(pos) = waiting.iter().position(|&x| x == idx) {
        waiting.remove(pos);
    }
}

fn expire_waiters(
    now: u64,
    waiting: &mut Vec<usize>,
    jobs: &mut [Job],
    events: &mut Vec<Event>,
    stats: &mut Stats,
) {
    let mut expired: Vec<(u64, u32, usize)> = waiting
        .iter()
        .filter_map(|&idx| {
            jobs[idx]
                .deadline
                .filter(|&d| d <= now)
                .map(|d| (d, jobs[idx].customer.id, idx))
        })
        .collect();
    expired.sort_by_key(|&(deadline, id, _)| (deadline, id));

    for (_, _, idx) in expired {
        remove_waiter(waiting, idx);
        let segment = now - jobs[idx].queue_since;
        jobs[idx].waited += segment;
        jobs[idx].spent_wait += segment;
        stats.wait_ticks += segment as u128;
        stats.reneged += 1;
        emit(
            events,
            now,
            jobs[idx].customer.id,
            Kind::Renege {
                waited: jobs[idx].waited,
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

    let mut seen = Vec::new();
    let mut deadlines = Vec::with_capacity(customers.len());

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
        deadlines.push(deadline);
    }

    let mut jobs: Vec<Job> = customers
        .iter()
        .enumerate()
        .map(|(i, customer)| Job {
            customer: customer.clone(),
            deadline: deadlines[i],
            remaining: customer.service,
            queue_since: customer.arrival,
            waited: 0,
            spent_wait: 0,
            pause_time: None,
            ever_started: false,
        })
        .collect();

    let mut arrivals: Vec<usize> = (0..customers.len()).collect();
    arrivals.sort_by(|&a, &b| {
        jobs[a]
            .customer
            .arrival
            .cmp(&jobs[b].customer.arrival)
            .then_with(|| jobs[a].customer.priority.cmp(&jobs[b].customer.priority))
            .then_with(|| jobs[b].customer.id.cmp(&jobs[a].customer.id))
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
    let mut cursor = 0usize;
    let mut now = 0u64;
    let mut last_dispatched: Option<u32> = None;

    while cursor < arrivals.len() || active.is_some() || !waiting.is_empty() {
        let next_arrival = arrivals
            .get(cursor)
            .map(|&idx| jobs[idx].customer.arrival);
        let next_finish = active.as_ref().map(|a| a.finish);
        let next_work_at = active.as_ref().map(|a| a.work_at).filter(|&t| t > now);
        let next_deadline = waiting
            .iter()
            .filter_map(|&idx| jobs[idx].deadline.filter(|&d| d > now))
            .min();

        let next = [
            next_arrival,
            next_finish,
            next_work_at,
            next_deadline,
        ]
        .into_iter()
        .flatten()
        .filter(|&t| t >= now)
        .min();

        let Some(tick) = next else {
            break;
        };
        now = tick;

        // Phase A: finish an active customer before other work at this tick.
        if active.as_ref().is_some_and(|a| a.finish == now) {
            let finished = active.take().unwrap();
            let idx = finished.job;
            let id = jobs[idx].customer.id;
            stats.busy_ticks += (now - finished.dispatch_time) as u128;
            stats.completed += 1;
            stats.turnaround_ticks += (now - jobs[idx].customer.arrival) as u128;
            stats.completion_order.push(id);
            emit(&mut events, now, id, Kind::Finish);
        }

        // Phase B: expire customers who were waiting at the start of this tick.
        expire_waiters(now, &mut waiting, &mut jobs, &mut events, &mut stats);

        // Phase C: process the entire arrival cohort without dispatching.
        while cursor < arrivals.len()
            && jobs[arrivals[cursor]].customer.arrival == now
        {
            let idx = arrivals[cursor];
            cursor += 1;
            let id = jobs[idx].customer.id;
            emit(&mut events, now, id, Kind::Arrive);

            let server_idle = active.is_none();
            let limit = config.waiting_capacity + usize::from(server_idle);
            if waiting.len() < limit {
                stats.accepted += 1;
                waiting.push(idx);
            } else {
                stats.balked += 1;
                emit(&mut events, now, id, Kind::Balk);
            }
        }

        // Phase D: consider at most one preemption after protection ends.
        let protection_ended = active
            .as_ref()
            .map_or(true, |a| now >= a.work_at);
        if protection_ended {
            if let (Some(current), Some(waiter)) =
                (active.as_ref().map(|a| a.job), best_waiter(&waiting, &jobs))
            {
                if jobs[waiter].customer.priority < jobs[current].customer.priority {
                    let running = active.take().unwrap();
                    let idx = running.job;
                    let id = jobs[idx].customer.id;
                    let by = jobs[waiter].customer.id;
                    let remaining = running.finish - now;
                    stats.busy_ticks += (now - running.dispatch_time) as u128;
                    emit(
                        &mut events,
                        now,
                        id,
                        Kind::Preempt { remaining, by },
                    );
                    stats.preemptions += 1;

                    jobs[idx].remaining = remaining;
                    jobs[idx].queue_since = now;
                    jobs[idx].pause_time = Some(now);

                    if let Some(budget) = jobs[idx].customer.patience {
                        let consumed = jobs[idx].spent_wait;
                        let unconsumed = budget.saturating_sub(consumed);
                        jobs[idx].deadline = Some(now.checked_add(unconsumed).ok_or(
                            Error::TimeOverflow {
                                customer: id,
                                time: now,
                                stage: OverflowStage::Patience,
                            },
                        )?);
                    }

                    waiting.push(idx);
                }
            }
        }

        // Phase E: dispatch while the server is idle, handling inline finishes.
        loop {
            if active.is_some() {
                break;
            }
            let Some(idx) = best_waiter(&waiting, &jobs) else {
                break;
            };
            remove_waiter(&mut waiting, idx);

            let id = jobs[idx].customer.id;
            let wait_segment = now - jobs[idx].queue_since;
            jobs[idx].waited += wait_segment;
            jobs[idx].spent_wait += wait_segment;
            stats.wait_ticks += wait_segment as u128;

            if let Some(budget) = jobs[idx].customer.patience {
                let remaining_budget = budget.saturating_sub(jobs[idx].spent_wait);
                jobs[idx].deadline = now.checked_add(remaining_budget);
            }

            let cost = if last_dispatched.is_some_and(|last| last != id) {
                config.switch_cost
            } else {
                0
            };
            let work_at = now.checked_add(cost).ok_or(Error::TimeOverflow {
                customer: id,
                time: now,
                stage: OverflowStage::WorkAt,
            })?;
            let finish = work_at
                .checked_add(jobs[idx].remaining)
                .ok_or(Error::TimeOverflow {
                    customer: id,
                    time: now,
                    stage: OverflowStage::Finish,
                })?;

            let remaining = jobs[idx].remaining;
            let kind = if jobs[idx].ever_started {
                Kind::Resume {
                    remaining,
                    work_at,
                    finish,
                }
            } else {
                jobs[idx].ever_started = true;
                stats.started += 1;
                Kind::Start {
                    remaining,
                    work_at,
                    finish,
                }
            };
            emit(&mut events, now, id, kind);
            last_dispatched = Some(id);

            active = Some(Active {
                job: idx,
                work_at,
                finish,
                dispatch_time: now,
            });

            if finish == now {
                active = None;
                stats.completed += 1;
                stats.turnaround_ticks += (now - jobs[idx].customer.arrival) as u128;
                stats.completion_order.push(id);
                emit(&mut events, now, id, Kind::Finish);
                continue;
            }
            break;
        }

        // Phase F: expire any waiters queued or paused during this tick.
        expire_waiters(now, &mut waiting, &mut jobs, &mut events, &mut stats);
    }

    stats.elapsed = events.last().map_or(0, |event| event.time);
    stats.mean_wait_milli = if stats.accepted == 0 {
        0
    } else {
        rounded_ratio(1000 * stats.wait_ticks, stats.accepted as u128).unwrap_or(0)
    };
    stats.mean_turnaround_milli = if stats.completed == 0 {
        0
    } else {
        rounded_ratio(
            1000 * stats.turnaround_ticks,
            stats.completed as u128,
        )
        .unwrap_or(0)
    };
    stats.utilization_bps = if stats.elapsed == 0 {
        0
    } else {
        rounded_ratio(10000 * stats.busy_ticks, stats.elapsed as u128)
            .unwrap_or(0)
            .min(u32::MAX as u128) as u32
    };

    Ok(Report { events, stats })
}