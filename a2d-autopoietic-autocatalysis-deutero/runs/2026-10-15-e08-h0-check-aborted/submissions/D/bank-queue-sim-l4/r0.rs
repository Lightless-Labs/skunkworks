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
    queued_at: u64,
    waited: u64,
    started: bool,
    paused: bool,
    pause_time: u64,
}

#[derive(Clone)]
struct Active {
    index: usize,
    dispatched_at: u64,
    work_at: u64,
    finish: u64,
}

fn best_waiter(waiting: &[usize], jobs: &[Job]) -> Option<usize> {
    waiting.iter().copied().min_by(|&a, &b| {
        let x = &jobs[a];
        let y = &jobs[b];

        x.customer
            .priority
            .cmp(&y.customer.priority)
            .then_with(|| y.paused.cmp(&x.paused))
            .then_with(|| {
                if x.paused && y.paused {
                    x.pause_time.cmp(&y.pause_time)
                } else {
                    x.customer.arrival.cmp(&y.customer.arrival)
                }
            })
            .then_with(|| {
                if x.paused && y.paused {
                    x.customer.id.cmp(&y.customer.id)
                } else {
                    y.customer.id.cmp(&x.customer.id)
                }
            })
    })
}

fn emit(events: &mut Vec<Event>, time: u64, customer: u32, kind: Kind) {
    events.push(Event { time, customer, kind });
}

fn expire_waiters(
    now: u64,
    waiting: &mut Vec<usize>,
    jobs: &mut [Job],
    events: &mut Vec<Event>,
    reneged: &mut usize,
    wait_ticks: &mut u128,
) {
    let mut expired: Vec<usize> = waiting
        .iter()
        .copied()
        .filter(|&i| jobs[i].deadline.is_some_and(|d| d <= now))
        .collect();

    expired.sort_by(|&a, &b| {
        jobs[a]
            .deadline
            .cmp(&jobs[b].deadline)
            .then_with(|| jobs[a].customer.id.cmp(&jobs[b].customer.id))
    });

    for idx in expired {
        if let Some(pos) = waiting.iter().position(|&x| x == idx) {
            waiting.remove(pos);
            let segment = now - jobs[idx].queued_at;
            jobs[idx].waited += segment;
            *wait_ticks += segment as u128;
            emit(
                events,
                now,
                jobs[idx].customer.id,
                Kind::Renege {
                    waited: jobs[idx].waited,
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

    let mut jobs: Vec<Job> = customers
        .iter()
        .enumerate()
        .map(|(i, customer)| Job {
            customer: customer.clone(),
            deadline: initial_deadlines[i],
            remaining: customer.service,
            queued_at: customer.arrival,
            waited: 0,
            started: false,
            paused: false,
            pause_time: 0,
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

    let mut events = Vec::new();
    let mut waiting = Vec::new();
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
    let mut last_event_time = 0u64;

    loop {
        // Phase A: finish before expiration or arrivals.
        if active.as_ref().is_some_and(|a| a.finish == now) {
            let a = active.take().unwrap();
            let id = jobs[a.index].customer.id;
            emit(&mut events, now, id, Kind::Finish);
            last_event_time = now;
            completed += 1;
            turnaround_ticks += (now - jobs[a.index].customer.arrival) as u128;
            completion_order.push(id);
            busy_ticks += (now - a.dispatched_at) as u128;
        }

        // Phase B: expire waiters that were already queued.
        expire_waiters(
            now,
            &mut waiting,
            &mut jobs,
            &mut events,
            &mut reneged,
            &mut wait_ticks,
        );

        // Phase C: process the complete arrival cohort without dispatching.
        while arrival_pos < arrivals.len()
            && jobs[arrivals[arrival_pos]].customer.arrival == now
        {
            let idx = arrivals[arrival_pos];
            arrival_pos += 1;
            let id = jobs[idx].customer.id;
            emit(&mut events, now, id, Kind::Arrive);
            last_event_time = now;

            let gate = config.waiting_capacity + usize::from(active.is_none());
            if waiting.len() >= gate {
                emit(&mut events, now, id, Kind::Balk);
                last_event_time = now;
                balked += 1;
            } else {
                jobs[idx].queued_at = now;
                jobs[idx].remaining = jobs[idx].customer.service;
                waiting.push(idx);
                accepted += 1;
            }
        }

        // Phase D: after switching protection ends, allow one preemption.
        if let Some(a) = active.as_ref() {
            if now >= a.work_at {
                if let Some(waiter) = best_waiter(&waiting, &jobs) {
                    if jobs[waiter].customer.priority < jobs[a.index].customer.priority {
                        let a = active.take().unwrap();
                        let old = a.index;
                        let remaining = a.finish - now;
                        let id = jobs[old].customer.id;
                        let by = jobs[waiter].customer.id;

                        emit(&mut events, now, id, Kind::Preempt { remaining, by });
                        last_event_time = now;
                        preemptions += 1;
                        busy_ticks += (now - a.dispatched_at) as u128;

                        jobs[old].remaining = remaining;
                        jobs[old].queued_at = now;
                        jobs[old].paused = true;
                        jobs[old].pause_time = now;
                        jobs[old].deadline = match jobs[old].customer.patience {
                            Some(budget) => Some(now.checked_add(budget).ok_or(
                                Error::TimeOverflow {
                                    customer: id,
                                    time: now,
                                    stage: OverflowStage::Patience,
                                },
                            )?),
                            None => None,
                        };
                        waiting.push(old);
                    }
                }
            }
        }

        // Phase E: dispatch until an active job remains or the queue is empty.
        loop {
            if active.is_some() {
                break;
            }

            let Some(idx) = best_waiter(&waiting, &jobs) else {
                break;
            };
            let pos = waiting.iter().position(|&x| x == idx).unwrap();
            waiting.remove(pos);

            let segment = now - jobs[idx].queued_at;
            jobs[idx].waited += segment;
            wait_ticks += segment as u128;

            let id = jobs[idx].customer.id;
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
                .checked_add(jobs[idx].remaining)
                .ok_or(Error::TimeOverflow {
                    customer: id,
                    time: now,
                    stage: OverflowStage::Finish,
                })?;

            if jobs[idx].started {
                emit(
                    &mut events,
                    now,
                    id,
                    Kind::Resume {
                        remaining: jobs[idx].remaining,
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
                        remaining: jobs[idx].remaining,
                        work_at,
                        finish,
                    },
                );
                jobs[idx].started = true;
                started += 1;
            }
            last_event_time = now;
            last_dispatched = Some(id);
            active = Some(Active {
                index: idx,
                dispatched_at: now,
                work_at,
                finish,
            });

            if finish == now {
                let a = active.take().unwrap();
                emit(&mut events, now, id, Kind::Finish);
                last_event_time = now;
                completed += 1;
                turnaround_ticks += (now - jobs[idx].customer.arrival) as u128;
                completion_order.push(id);
                busy_ticks += (now - a.dispatched_at) as u128;
                continue;
            }

            break;
        }

        // Phase F: expire waiters queued during this tick, after dispatches.
        expire_waiters(
            now,
            &mut waiting,
            &mut jobs,
            &mut events,
            &mut reneged,
            &mut wait_ticks,
        );

        if completed + reneged + balked == customers.len() {
            break;
        }

        let mut next: Option<u64> = None;
        if arrival_pos < arrivals.len() {
            next = Some(jobs[arrivals[arrival_pos]].customer.arrival);
        }

        if let Some(a) = active.as_ref() {
            for t in [Some(a.finish), (a.work_at > now).then_some(a.work_at)]
                .into_iter()
                .flatten()
            {
                if t > now {
                    next = Some(next.map_or(t, |current| current.min(t)));
                }
            }
        }

        for &idx in &waiting {
            if let Some(deadline) = jobs[idx].deadline {
                if deadline > now {
                    next = Some(next.map_or(deadline, |current| current.min(deadline)));
                }
            }
        }

        if let Some(t) = next {
            now = t;
        } else {
            break;
        }
    }

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
    let elapsed = last_event_time;
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