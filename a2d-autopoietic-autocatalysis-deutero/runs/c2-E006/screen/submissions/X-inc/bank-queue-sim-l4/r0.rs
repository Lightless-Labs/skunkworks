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
    let complement = denominator - remainder;

    let round_up = remainder > complement || (remainder == complement && quotient % 2 == 1);
    Some(if round_up { quotient + 1 } else { quotient })
}

#[derive(Clone)]
struct Job {
    id: u32,
    arrival: u64,
    priority: u8,
    remaining: u64,
    patience_left: Option<u64>,
    deadline: Option<u64>,
    entry_time: u64,
    waited: u64,
    started: bool,
    paused: bool,
    pause_time: u64,
}

#[derive(Clone)]
struct Active {
    job: Job,
    dispatch_time: u64,
    work_at: u64,
    finish: u64,
}

fn job_better(a: &Job, b: &Job) -> bool {
    if a.priority != b.priority {
        return a.priority < b.priority;
    }
    if a.paused != b.paused {
        return a.paused;
    }
    if a.paused {
        if a.pause_time != b.pause_time {
            return a.pause_time < b.pause_time;
        }
        return a.id < b.id;
    }
    if a.arrival != b.arrival {
        return a.arrival < b.arrival;
    }
    a.id > b.id
}

fn best_waiter(waiting: &[Job]) -> Option<usize> {
    let mut best = None;
    for i in 0..waiting.len() {
        if best.map_or(true, |j| job_better(&waiting[i], &waiting[j])) {
            best = Some(i);
        }
    }
    best
}

fn emit(events: &mut Vec<Event>, time: u64, customer: u32, kind: Kind) {
    events.push(Event {
        time,
        customer,
        kind,
    });
}

fn expire_waiters(waiting: &mut Vec<Job>, now: u64, events: &mut Vec<Event>, stats: &mut Stats) {
    let mut expired: Vec<Job> = Vec::new();
    let mut retained = Vec::with_capacity(waiting.len());

    for job in waiting.drain(..) {
        if job.deadline.map_or(false, |deadline| deadline <= now) {
            expired.push(job);
        } else {
            retained.push(job);
        }
    }

    expired.sort_by(|a, b| {
        a.deadline
            .cmp(&b.deadline)
            .then_with(|| a.id.cmp(&b.id))
    });

    for mut job in expired {
        let segment = now - job.entry_time;
        job.waited = job.waited.saturating_add(segment);
        stats.wait_ticks += segment as u128;
        stats.reneged += 1;
        emit(
            events,
            now,
            job.id,
            Kind::Renege {
                waited: job.waited,
            },
        );
    }

    *waiting = retained;
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

    for (index, customer) in customers.iter().enumerate() {
        if customer.id == 0 {
            return Err(Error::ZeroId { index });
        }
        if customers[..index]
            .iter()
            .any(|previous| previous.id == customer.id)
        {
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
        if let Some(patience) = customer.patience {
            if customer.arrival.checked_add(patience).is_none() {
                return Err(Error::DeadlineOverflow {
                    index,
                    id: customer.id,
                });
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
    let mut waiting: Vec<Job> = Vec::new();
    let mut active: Option<Active> = None;
    let mut next_arrival = 0usize;
    let mut now = 0u64;
    let mut last_dispatched: Option<u32> = None;
    let mut last_event_time = 0u64;

    loop {
        // Advance directly to the next externally observable or internal boundary.
        if events.is_empty() && active.is_none() && waiting.is_empty() {
            if next_arrival >= arrivals.len() {
                break;
            }
            now = customers[arrivals[next_arrival]].arrival;
        }

        // Phase A: a finish at this boundary precedes all other work.
        if let Some(current) = active.as_ref() {
            if current.finish == now {
                let finished = active.take().unwrap();
                let id = finished.job.id;
                stats.busy_ticks += (now - finished.dispatch_time) as u128;
                stats.completed += 1;
                stats.turnaround_ticks += (now - finished.job.arrival) as u128;
                stats.completion_order.push(id);
                emit(&mut events, now, id, Kind::Finish);
                last_event_time = now;
            }
        }

        // Phase B: expire customers that were already waiting at the start of the tick.
        expire_waiters(&mut waiting, now, &mut events, &mut stats);
        if events.last().map(|e| e.time == now).unwrap_or(false) {
            last_event_time = now;
        }

        // Phase C: process the entire arrival cohort without dispatching between arrivals.
        while next_arrival < arrivals.len()
            && customers[arrivals[next_arrival]].arrival == now
        {
            let customer = &customers[arrivals[next_arrival]];
            next_arrival += 1;
            emit(&mut events, now, customer.id, Kind::Arrive);
            last_event_time = now;

            let gate = config.waiting_capacity + usize::from(active.is_none());
            if waiting.len() >= gate {
                stats.balked += 1;
                emit(&mut events, now, customer.id, Kind::Balk);
                last_event_time = now;
                continue;
            }

            stats.accepted += 1;
            let deadline = customer
                .patience
                .map(|p| customer.arrival + p);
            waiting.push(Job {
                id: customer.id,
                arrival: customer.arrival,
                priority: customer.priority,
                remaining: customer.service,
                patience_left: customer.patience,
                deadline,
                entry_time: now,
                waited: 0,
                started: false,
                paused: false,
                pause_time: now,
            });
        }

        // Phase D: reconsider the active customer once protection has ended.
        if let Some(current) = active.as_ref() {
            if now >= current.work_at {
                if let Some(index) = best_waiter(&waiting) {
                    if waiting[index].priority < current.job.priority {
                        let waiter_id = waiting[index].id;
                        let mut paused = active.take().unwrap();
                        let remaining = paused.finish - now;
                        stats.busy_ticks += (now - paused.dispatch_time) as u128;
                        stats.preemptions += 1;
                        emit(
                            &mut events,
                            now,
                            paused.job.id,
                            Kind::Preempt {
                                remaining,
                                by: waiter_id,
                            },
                        );
                        last_event_time = now;

                        paused.job.remaining = remaining;
                        paused.job.paused = true;
                        paused.job.pause_time = now;
                        paused.job.entry_time = now;
                        paused.job.deadline = match paused.job.patience_left {
                            Some(budget) => Some(now.checked_add(budget).ok_or(
                                Error::TimeOverflow {
                                    customer: paused.job.id,
                                    time: now,
                                    stage: OverflowStage::Patience,
                                },
                            )?),
                            None => None,
                        };
                        waiting.push(paused.job);
                    }
                }
            }
        }

        // Phase E: dispatch while idle. Inline zero-length completions remain in this phase.
        loop {
            if active.is_some() {
                break;
            }
            let Some(index) = best_waiter(&waiting) else {
                break;
            };

            let mut job = waiting.remove(index);
            let segment = now - job.entry_time;
            job.waited = job.waited.saturating_add(segment);
            stats.wait_ticks += segment as u128;
            if let Some(left) = job.patience_left.as_mut() {
                *left -= segment;
            }

            let switch = if last_dispatched == Some(job.id) {
                0
            } else if last_dispatched.is_none() {
                0
            } else {
                config.switch_cost
            };

            let work_at = now.checked_add(switch).ok_or(Error::TimeOverflow {
                customer: job.id,
                time: now,
                stage: OverflowStage::WorkAt,
            })?;
            let finish = work_at
                .checked_add(job.remaining)
                .ok_or(Error::TimeOverflow {
                    customer: job.id,
                    time: now,
                    stage: OverflowStage::Finish,
                })?;

            let was_started = job.started;
            let kind = if was_started {
                Kind::Resume {
                    remaining: job.remaining,
                    work_at,
                    finish,
                }
            } else {
                Kind::Start {
                    remaining: job.remaining,
                    work_at,
                    finish,
                }
            };
            emit(&mut events, now, job.id, kind);
            last_event_time = now;
            if !was_started {
                stats.started += 1;
                job.started = true;
            }
            last_dispatched = Some(job.id);

            active = Some(Active {
                job,
                dispatch_time: now,
                work_at,
                finish,
            });

            if finish == now {
                let finished = active.take().unwrap();
                let id = finished.job.id;
                stats.completed += 1;
                stats.turnaround_ticks += (now - finished.job.arrival) as u128;
                stats.completion_order.push(id);
                emit(&mut events, now, id, Kind::Finish);
                last_event_time = now;
                continue;
            }
            break;
        }

        // Phase F: expire newly queued customers, including zero-budget arrivals/pauses.
        expire_waiters(&mut waiting, now, &mut events, &mut stats);
        if events.last().map(|e| e.time == now).unwrap_or(false) {
            last_event_time = now;
        }

        if next_arrival >= arrivals.len() && active.is_none() && waiting.is_empty() {
            break;
        }

        let mut next_time: Option<u64> = None;
        let mut consider = |candidate: u64| {
            if candidate > now && next_time.map_or(true, |old| candidate < old) {
                next_time = Some(candidate);
            }
        };

        if next_arrival < arrivals.len() {
            consider(customers[arrivals[next_arrival]].arrival);
        }
        if let Some(current) = active.as_ref() {
            consider(current.finish);
            if current.work_at > now {
                consider(current.work_at);
            }
        }
        for job in &waiting {
            if let Some(deadline) = job.deadline {
                consider(deadline);
            }
        }

        if let Some(next) = next_time {
            now = next;
        } else {
            // Any remaining waiter can be dispatched at this same tick on the next pass.
            // This fallback also prevents a malformed state from causing tick-by-tick work.
            if active.is_none() && !waiting.is_empty() {
                continue;
            }
            break;
        }
    }

    stats.elapsed = last_event_time;
    stats.mean_wait_milli = if stats.accepted == 0 {
        0
    } else {
        rounded_ratio(stats.wait_ticks.saturating_mul(1000), stats.accepted as u128).unwrap()
    };
    stats.mean_turnaround_milli = if stats.completed == 0 {
        0
    } else {
        rounded_ratio(
            stats.turnaround_ticks.saturating_mul(1000),
            stats.completed as u128,
        )
        .unwrap()
    };
    stats.utilization_bps = if stats.elapsed == 0 {
        0
    } else {
        rounded_ratio(stats.busy_ticks.saturating_mul(10000), stats.elapsed as u128)
            .unwrap() as u32
    };

    Ok(Report { events, stats })
}