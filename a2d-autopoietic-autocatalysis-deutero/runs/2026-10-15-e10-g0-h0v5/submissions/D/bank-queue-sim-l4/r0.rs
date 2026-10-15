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
    let complement = denominator - remainder;

    Some(if remainder > complement || (remainder == complement && quotient & 1 == 1) {
        quotient + 1
    } else {
        quotient
    })
}

#[derive(Clone)]
struct Job {
    customer: Customer,
    deadline: Option<u64>,
    remaining: u64,
    queue_since: u64,
    waited: u64,
    paused: bool,
    pause_time: u64,
    ever_started: bool,
}

struct Active {
    idx: usize,
    work_at: u64,
    finish: u64,
    dispatch_at: u64,
}

fn push_event(events: &mut Vec<Event>, time: u64, customer: u32, kind: Kind) {
    events.push(Event { time, customer, kind });
}

fn better(a: &Job, b: &Job) -> bool {
    if a.customer.priority != b.customer.priority {
        return a.customer.priority < b.customer.priority;
    }
    if a.paused != b.paused {
        return a.paused;
    }
    if a.paused {
        a.pause_time
            .cmp(&b.pause_time)
            .then_with(|| a.customer.id.cmp(&b.customer.id))
            .is_lt()
    } else {
        a.customer
            .arrival
            .cmp(&b.customer.arrival)
            .then_with(|| b.customer.id.cmp(&a.customer.id))
            .is_lt()
    }
}

fn best_waiter(waiting: &[usize], jobs: &[Job]) -> Option<usize> {
    waiting
        .iter()
        .copied()
        .min_by(|&a, &b| {
            if better(&jobs[a], &jobs[b]) {
                std::cmp::Ordering::Less
            } else if better(&jobs[b], &jobs[a]) {
                std::cmp::Ordering::Greater
            } else {
                std::cmp::Ordering::Equal
            }
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
    waiting: &mut Vec<usize>,
    jobs: &mut [Job],
    events: &mut Vec<Event>,
    wait_ticks: &mut u128,
    reneged: &mut usize,
) {
    let mut expired: Vec<usize> = waiting
        .iter()
        .copied()
        .filter(|&i| jobs[i].deadline.is_some_and(|deadline| deadline <= now))
        .collect();

    expired.sort_by(|&a, &b| {
        jobs[a]
            .deadline
            .cmp(&jobs[b].deadline)
            .then_with(|| jobs[a].customer.id.cmp(&jobs[b].customer.id))
    });

    for i in expired {
        if let Some(pos) = waiting.iter().position(|&x| x == i) {
            waiting.remove(pos);
            let segment = now - jobs[i].queue_since;
            jobs[i].waited += segment;
            *wait_ticks += segment as u128;
            push_event(
                events,
                now,
                jobs[i].customer.id,
                Kind::Renege {
                    waited: jobs[i].waited,
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

    let mut seen = Vec::with_capacity(customers.len());
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
            Some(patience) => Some(customer.arrival.checked_add(patience).ok_or(
                Error::DeadlineOverflow {
                    index,
                    id: customer.id,
                },
            )?),
            None => None,
        };
        deadlines.push(deadline);
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
            deadline: deadlines[i],
            remaining: customer.service,
            queue_since: customer.arrival,
            waited: 0,
            paused: false,
            pause_time: 0,
            ever_started: false,
        })
        .collect();

    let mut events = Vec::new();
    let mut waiting: Vec<usize> = Vec::new();
    let mut active: Option<Active> = None;
    let mut now = 0u64;
    let mut next_arrival = 0usize;
    let mut last_dispatched: Option<u32> = None;
    let mut any_dispatch = false;

    let (mut accepted, mut balked, mut reneged, mut completed, mut started, mut preemptions) =
        (0usize, 0usize, 0usize, 0usize, 0usize, 0usize);
    let (mut busy_ticks, mut wait_ticks, mut turnaround_ticks) = (0u128, 0u128, 0u128);
    let mut completion_order = Vec::new();

    while next_arrival < order.len() || active.is_some() || !waiting.is_empty() {
        if active.is_none() && waiting.is_empty() && next_arrival < order.len() {
            now = now.max(customers[order[next_arrival]].arrival);
        }

        // Phase A: a completion at this boundary occurs before all other work.
        if active.as_ref().is_some_and(|a| a.finish == now) {
            let a = active.take().unwrap();
            let id = jobs[a.idx].customer.id;
            busy_ticks += (now - a.dispatch_at) as u128;
            push_event(&mut events, now, id, Kind::Finish);
            completed += 1;
            turnaround_ticks += (now - jobs[a.idx].customer.arrival) as u128;
            completion_order.push(id);
        }

        // Phase B: expire customers who were already waiting at tick start.
        expire_waiters(
            now,
            &mut waiting,
            &mut jobs,
            &mut events,
            &mut wait_ticks,
            &mut reneged,
        );

        // Phase C: process the entire arrival cohort without dispatching.
        while next_arrival < order.len() && customers[order[next_arrival]].arrival == now {
            let i = order[next_arrival];
            next_arrival += 1;
            let id = jobs[i].customer.id;
            push_event(&mut events, now, id, Kind::Arrive);

            let limit = config.waiting_capacity + usize::from(active.is_none());
            if waiting.len() >= limit {
                push_event(&mut events, now, id, Kind::Balk);
                balked += 1;
            } else {
                accepted += 1;
                jobs[i].queue_since = now;
                waiting.push(i);
            }
        }

        // Phase D: only an active customer past its switch protection can be preempted.
        if let Some(a) = active.as_ref() {
            if now >= a.work_at {
                if let Some(w) = best_waiter(&waiting, &jobs) {
                    if jobs[w].customer.priority < jobs[a.idx].customer.priority {
                        let a = active.take().unwrap();
                        let id = jobs[a.idx].customer.id;
                        let remaining = a.finish - now;

                        push_event(
                            &mut events,
                            now,
                            id,
                            Kind::Preempt {
                                remaining,
                                by: jobs[w].customer.id,
                            },
                        );
                        preemptions += 1;
                        busy_ticks += (now - a.dispatch_at) as u128;

                        jobs[a.idx].remaining = remaining;
                        jobs[a.idx].queue_since = now;
                        jobs[a.idx].paused = true;
                        jobs[a.idx].pause_time = now;

                        if let Some(budget) = jobs[a.idx].customer.patience {
                            let remaining_budget = budget.saturating_sub(jobs[a.idx].waited);
                            jobs[a.idx].deadline = Some(
                                now.checked_add(remaining_budget).ok_or_else(|| {
                                    overflow(id, now, OverflowStage::Patience)
                                })?,
                            );
                        }

                        waiting.push(a.idx);
                    }
                }
            }
        }

        // Phase E: dispatch until one customer remains active or the queue is empty.
        loop {
            if active.is_some() {
                break;
            }

            let Some(i) = best_waiter(&waiting, &jobs) else {
                break;
            };
            let pos = waiting.iter().position(|&x| x == i).unwrap();
            waiting.remove(pos);

            let segment = now - jobs[i].queue_since;
            jobs[i].waited += segment;
            wait_ticks += segment as u128;

            let id = jobs[i].customer.id;
            let cost = if !any_dispatch || last_dispatched == Some(id) {
                0
            } else {
                config.switch_cost
            };
            let work_at = now
                .checked_add(cost)
                .ok_or_else(|| overflow(id, now, OverflowStage::WorkAt))?;
            let finish = work_at
                .checked_add(jobs[i].remaining)
                .ok_or_else(|| overflow(id, now, OverflowStage::Finish))?;

            let was_started = jobs[i].ever_started;
            let kind = if was_started {
                Kind::Resume {
                    remaining: jobs[i].remaining,
                    work_at,
                    finish,
                }
            } else {
                started += 1;
                Kind::Start {
                    remaining: jobs[i].remaining,
                    work_at,
                    finish,
                }
            };
            jobs[i].ever_started = true;
            any_dispatch = true;
            last_dispatched = Some(id);
            push_event(&mut events, now, id, kind);

            active = Some(Active {
                idx: i,
                work_at,
                finish,
                dispatch_at: now,
            });

            if finish == now {
                let a = active.take().unwrap();
                busy_ticks += (now - a.dispatch_at) as u128;
                push_event(&mut events, now, id, Kind::Finish);
                completed += 1;
                turnaround_ticks += (now - jobs[i].customer.arrival) as u128;
                completion_order.push(id);
                continue;
            }
            break;
        }

        // Phase F: also expire customers queued during this tick or just paused.
        expire_waiters(
            now,
            &mut waiting,
            &mut jobs,
            &mut events,
            &mut wait_ticks,
            &mut reneged,
        );

        if next_arrival >= order.len() && active.is_none() && waiting.is_empty() {
            break;
        }

        let mut next_time: Option<u64> = None;
        let mut consider = |time: u64| {
            if time > now && next_time.is_none_or(|current| time < current) {
                next_time = Some(time);
            }
        };

        if next_arrival < order.len() {
            consider(customers[order[next_arrival]].arrival);
        }
        if let Some(a) = active.as_ref() {
            consider(a.finish);
            consider(a.work_at);
        }
        for &i in &waiting {
            if let Some(deadline) = jobs[i].deadline {
                consider(deadline);
            }
        }

        if let Some(time) = next_time {
            now = time;
        } else {
            break;
        }
    }

    let mean_wait_milli = if accepted == 0 {
        0
    } else {
        rounded_ratio(wait_ticks * 1000, accepted as u128).unwrap()
    };
    let mean_turnaround_milli = if completed == 0 {
        0
    } else {
        rounded_ratio(turnaround_ticks * 1000, completed as u128).unwrap()
    };
    let elapsed = events.last().map_or(0, |event| event.time);
    let utilization_bps = if elapsed == 0 {
        0
    } else {
        rounded_ratio(busy_ticks * 10_000, elapsed as u128)
            .unwrap()
            .min(u32::MAX as u128) as u32
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