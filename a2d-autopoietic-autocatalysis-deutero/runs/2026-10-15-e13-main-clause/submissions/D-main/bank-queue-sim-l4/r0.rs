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
    entry: u64,
    waited: u64,
    remaining: u64,
    started: bool,
    paused: bool,
    pause_time: u64,
}

#[derive(Clone, Copy)]
struct Active {
    index: usize,
    work_at: u64,
    finish: u64,
    interval_start: u64,
}

fn better(a: &Job, b: &Job) -> std::cmp::Ordering {
    a.customer
        .priority
        .cmp(&b.customer.priority)
        .then_with(|| match (a.paused, b.paused) {
            (true, false) => std::cmp::Ordering::Less,
            (false, true) => std::cmp::Ordering::Greater,
            (true, true) => a
                .pause_time
                .cmp(&b.pause_time)
                .then_with(|| a.customer.id.cmp(&b.customer.id)),
            (false, false) => a
                .customer
                .arrival
                .cmp(&b.customer.arrival)
                .then_with(|| b.customer.id.cmp(&a.customer.id)),
        })
}

fn overflow(customer: u32, time: u64, stage: OverflowStage) -> Error {
    Error::TimeOverflow {
        customer,
        time,
        stage,
    }
}

fn expire_waiters(
    now: u64,
    jobs: &mut [Job],
    waiting: &mut Vec<usize>,
    events: &mut Vec<Event>,
    stats: &mut Stats,
) {
    let mut expiring: Vec<usize> = waiting
        .iter()
        .copied()
        .filter(|&i| jobs[i].deadline.is_some_and(|d| d <= now))
        .collect();

    expiring.sort_by(|&a, &b| {
        jobs[a]
            .deadline
            .cmp(&jobs[b].deadline)
            .then_with(|| jobs[a].customer.id.cmp(&jobs[b].customer.id))
    });

    for i in expiring {
        if let Some(pos) = waiting.iter().position(|&x| x == i) {
            waiting.remove(pos);
            let segment = now - jobs[i].entry;
            jobs[i].waited += segment;
            stats.wait_ticks += segment as u128;
            events.push(Event {
                time: now,
                customer: jobs[i].customer.id,
                kind: Kind::Renege {
                    waited: jobs[i].waited,
                },
            });
            stats.reneged += 1;
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

    let mut order: Vec<usize> = (0..customers.len()).collect();
    order.sort_by(|&a, &b| {
        customers[a]
            .arrival
            .cmp(&customers[b].arrival)
            .then_with(|| customers[a].priority.cmp(&customers[b].priority))
            .then_with(|| customers[b].id.cmp(&customers[a].id))
    });

    let mut jobs: Vec<Job> = customers
        .iter()
        .enumerate()
        .map(|(i, customer)| Job {
            customer: customer.clone(),
            deadline: initial_deadlines[i],
            entry: customer.arrival,
            waited: 0,
            remaining: customer.service,
            started: false,
            paused: false,
            pause_time: 0,
        })
        .collect();

    let mut events = Vec::new();
    let mut waiting: Vec<usize> = Vec::new();
    let mut active: Option<Active> = None;
    let mut next_arrival = 0usize;
    let mut last_dispatch: Option<u32> = None;
    let mut first_dispatch = true;

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

    let mut now = customers.iter().map(|c| c.arrival).min().unwrap();

    loop {
        // Phase A: a completion takes precedence over all other work at this tick.
        if active.is_some_and(|a| a.finish == now) {
            let a = active.take().unwrap();
            let id = jobs[a.index].customer.id;
            stats.busy_ticks += (now - a.interval_start) as u128;
            events.push(Event {
                time: now,
                customer: id,
                kind: Kind::Finish,
            });
            stats.completed += 1;
            stats.turnaround_ticks +=
                (now - jobs[a.index].customer.arrival) as u128;
            stats.completion_order.push(id);
        }

        // Phase B: expire the waiters that were present at the start of the tick.
        expire_waiters(now, &mut jobs, &mut waiting, &mut events, &mut stats);

        // Phase C: process the full arrival cohort without dispatching between arrivals.
        while next_arrival < order.len()
            && customers[order[next_arrival]].arrival == now
        {
            let i = order[next_arrival];
            next_arrival += 1;

            events.push(Event {
                time: now,
                customer: jobs[i].customer.id,
                kind: Kind::Arrive,
            });

            let admission_limit =
                config.waiting_capacity + usize::from(active.is_none());
            if waiting.len() >= admission_limit {
                events.push(Event {
                    time: now,
                    customer: jobs[i].customer.id,
                    kind: Kind::Balk,
                });
                stats.balked += 1;
            } else {
                stats.accepted += 1;
                waiting.push(i);
            }
        }

        // Phase D: compare before re-enqueuing the preempted customer.
        if let Some(a) = active {
            if now >= a.work_at {
                let best = waiting.iter().copied().min_by(|&x, &y| {
                    better(&jobs[x], &jobs[y])
                });

                if let Some(best) = best {
                    if jobs[best].customer.priority < jobs[a.index].customer.priority {
                        let remaining = a.finish - now;
                        let paused_id = jobs[a.index].customer.id;
                        events.push(Event {
                            time: now,
                            customer: paused_id,
                            kind: Kind::Preempt {
                                remaining,
                                by: jobs[best].customer.id,
                            },
                        });
                        stats.preemptions += 1;
                        stats.busy_ticks += (now - a.interval_start) as u128;

                        let budget = jobs[a.index]
                            .customer
                            .patience
                            .map(|p| p - jobs[a.index].waited);
                        let deadline = match budget {
                            Some(budget) => Some(now.checked_add(budget).ok_or_else(|| {
                                overflow(paused_id, now, OverflowStage::Patience)
                            })?),
                            None => None,
                        };

                        let paused = &mut jobs[a.index];
                        paused.remaining = remaining;
                        paused.entry = now;
                        paused.deadline = deadline;
                        paused.paused = true;
                        paused.pause_time = now;
                        waiting.push(a.index);
                        active = None;
                    }
                }
            }
        }

        // Phase E: dispatch until occupied or no waiters remain. Inline finishes
        // can immediately enable another dispatch at this same tick.
        loop {
            if active.is_some() || waiting.is_empty() {
                break;
            }

            let pos = waiting
                .iter()
                .enumerate()
                .min_by(|left, right| {
                    let x = *left.1;
                    let y = *right.1;
                    better(&jobs[x], &jobs[y])
                })
                .map(|(position, _)| position)
                .unwrap();

            let i = waiting.remove(pos);
            let id = jobs[i].customer.id;
            let segment = now - jobs[i].entry;
            jobs[i].waited += segment;
            stats.wait_ticks += segment as u128;

            let switch = if first_dispatch || last_dispatch == Some(id) {
                0
            } else {
                config.switch_cost
            };
            let work_at = now
                .checked_add(switch)
                .ok_or_else(|| overflow(id, now, OverflowStage::WorkAt))?;
            let finish = work_at
                .checked_add(jobs[i].remaining)
                .ok_or_else(|| overflow(id, now, OverflowStage::Finish))?;

            let kind = if jobs[i].started {
                Kind::Resume {
                    remaining: jobs[i].remaining,
                    work_at,
                    finish,
                }
            } else {
                jobs[i].started = true;
                stats.started += 1;
                Kind::Start {
                    remaining: jobs[i].remaining,
                    work_at,
                    finish,
                }
            };

            events.push(Event {
                time: now,
                customer: id,
                kind,
            });

            first_dispatch = false;
            last_dispatch = Some(id);
            active = Some(Active {
                index: i,
                work_at,
                finish,
                interval_start: now,
            });

            if finish == now {
                let a = active.take().unwrap();
                stats.busy_ticks += (now - a.interval_start) as u128;
                events.push(Event {
                    time: now,
                    customer: id,
                    kind: Kind::Finish,
                });
                stats.completed += 1;
                stats.turnaround_ticks +=
                    (now - jobs[i].customer.arrival) as u128;
                stats.completion_order.push(id);
                continue;
            }
            break;
        }

        // Phase F: expire waiters that were newly queued or paused this tick.
        expire_waiters(now, &mut jobs, &mut waiting, &mut events, &mut stats);

        if next_arrival == order.len() && active.is_none() && waiting.is_empty() {
            break;
        }

        let mut next = None;
        if next_arrival < order.len() {
            next = Some(customers[order[next_arrival]].arrival);
        }

        if let Some(a) = active {
            for time in [
                Some(a.finish),
                (a.work_at > now).then_some(a.work_at),
            ]
            .into_iter()
            .flatten()
            {
                if time > now {
                    next = Some(next.map_or(time, |current| current.min(time)));
                }
            }
        }

        for &i in &waiting {
            if let Some(deadline) = jobs[i].deadline {
                if deadline > now {
                    next = Some(next.map_or(deadline, |current| current.min(deadline)));
                }
            }
        }

        let Some(next_time) = next else {
            break;
        };
        now = next_time;
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
        rounded_ratio(stats.busy_ticks * 10_000, stats.elapsed as u128)
            .unwrap()
            .min(u32::MAX as u128) as u32
    };

    Ok(Report { events, stats })
}

fn main() {}