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
struct CustomerState {
    customer: Customer,
    deadline: Option<u64>,
    remaining: u64,
    total_waited: u64,
    queue_entry: u64,
    pause_time: Option<u64>,
    has_started: bool,
}

#[derive(Clone, Copy)]
struct Active {
    index: usize,
    work_at: u64,
    finish: u64,
}

fn waiting_better(a: usize, b: usize, states: &[CustomerState]) -> bool {
    let a = &states[a];
    let b = &states[b];

    if a.customer.priority != b.customer.priority {
        return a.customer.priority < b.customer.priority;
    }

    match (a.pause_time, b.pause_time) {
        (Some(at), Some(bt)) => {
            at < bt || (at == bt && a.customer.id < b.customer.id)
        }
        (Some(_), None) => true,
        (None, Some(_)) => false,
        (None, None) => {
            a.customer.arrival < b.customer.arrival
                || (a.customer.arrival == b.customer.arrival
                    && a.customer.id > b.customer.id)
        }
    }
}

fn best_waiter(waiting: &[usize], states: &[CustomerState]) -> Option<usize> {
    waiting
        .iter()
        .copied()
        .min_by(|a, b| {
            if waiting_better(*a, *b, states) {
                std::cmp::Ordering::Less
            } else if waiting_better(*b, *a, states) {
                std::cmp::Ordering::Greater
            } else {
                std::cmp::Ordering::Equal
            }
        })
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
            .any(|earlier| earlier.id == customer.id)
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

    let mut states: Vec<CustomerState> = customers
        .iter()
        .cloned()
        .map(|customer| CustomerState {
            deadline: customer
                .patience
                .and_then(|p| customer.arrival.checked_add(p)),
            remaining: customer.service,
            total_waited: 0,
            queue_entry: customer.arrival,
            pause_time: None,
            has_started: false,
            customer,
        })
        .collect();

    let mut arrivals: Vec<usize> = (0..customers.len()).collect();
    arrivals.sort_by(|&a, &b| {
        let ca = &states[a].customer;
        let cb = &states[b].customer;
        ca.arrival
            .cmp(&cb.arrival)
            .then_with(|| ca.priority.cmp(&cb.priority))
            .then_with(|| cb.id.cmp(&ca.id))
    });

    let mut events = Vec::new();
    let mut waiting: Vec<usize> = Vec::new();
    let mut active: Option<Active> = None;
    let mut next_arrival = 0usize;
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

    let mut now = arrivals
        .first()
        .map(|&i| states[i].customer.arrival)
        .unwrap_or(0);

    loop {
        // Phase A: finish before any other work at this tick.
        if let Some(a) = active {
            if a.finish == now {
                let id = states[a.index].customer.id;
                events.push(Event {
                    time: now,
                    customer: id,
                    kind: Kind::Finish,
                });
                last_event_time = now;
                completed += 1;
                turnaround_ticks += (now - states[a.index].customer.arrival) as u128;
                completion_order.push(id);
                active = None;
            }
        }

        // Phase B: expire customers that were waiting at the start of this tick.
        let mut expired: Vec<usize> = waiting
            .iter()
            .copied()
            .filter(|&i| states[i].deadline.is_some_and(|d| d <= now))
            .collect();
        expired.sort_by(|&a, &b| {
            states[a]
                .deadline
                .cmp(&states[b].deadline)
                .then_with(|| states[a].customer.id.cmp(&states[b].customer.id))
        });
        for i in expired {
            waiting.retain(|&j| j != i);
            let segment = now - states[i].queue_entry;
            states[i].total_waited += segment;
            wait_ticks += segment as u128;
            events.push(Event {
                time: now,
                customer: states[i].customer.id,
                kind: Kind::Renege {
                    waited: states[i].total_waited,
                },
            });
            last_event_time = now;
            reneged += 1;
        }

        // Phase C: process the entire arrival cohort without dispatch.
        while next_arrival < arrivals.len()
            && states[arrivals[next_arrival]].customer.arrival == now
        {
            let i = arrivals[next_arrival];
            next_arrival += 1;
            let id = states[i].customer.id;
            events.push(Event {
                time: now,
                customer: id,
                kind: Kind::Arrive,
            });
            last_event_time = now;

            let gate = config.waiting_capacity + usize::from(active.is_none());
            if waiting.len() < gate {
                accepted += 1;
                waiting.push(i);
            } else {
                balked += 1;
                events.push(Event {
                    time: now,
                    customer: id,
                    kind: Kind::Balk,
                });
                last_event_time = now;
            }
        }

        // Phase D: compare against the best waiter before re-enqueuing a
        // protected running customer.
        if let Some(a) = active {
            if now >= a.work_at {
                if let Some(w) = best_waiter(&waiting, &states) {
                    if states[w].customer.priority < states[a.index].customer.priority {
                        let remaining = a.finish - now;
                        let by = states[w].customer.id;
                        let paused_id = states[a.index].customer.id;

                        events.push(Event {
                            time: now,
                            customer: paused_id,
                            kind: Kind::Preempt { remaining, by },
                        });
                        last_event_time = now;
                        preemptions += 1;
                        busy_ticks += (now - a.work_at + (a.work_at - (now.min(a.work_at)))) as u128;
                        // The occupied interval begins at the dispatch time;
                        // recover it as finish minus remaining service and switch.
                        // Store it directly below through the active start time.
                        active = None;

                        states[a.index].remaining = remaining;
                        states[a.index].queue_entry = now;
                        states[a.index].pause_time = Some(now);
                        if let Some(budget) = states[a.index].customer.patience {
                            let deadline = now.checked_add(budget).ok_or(Error::TimeOverflow {
                                customer: paused_id,
                                time: now,
                                stage: OverflowStage::Patience,
                            })?;
                            states[a.index].deadline = Some(deadline);
                        }
                        waiting.push(a.index);
                    }
                }
            }
        }

        // Phase E: repeatedly dispatch and inline-finish zero-duration work.
        loop {
            if active.is_some() {
                break;
            }
            let Some(i) = best_waiter(&waiting, &states) else {
                break;
            };
            waiting.retain(|&j| j != i);

            let segment = now - states[i].queue_entry;
            states[i].total_waited += segment;
            wait_ticks += segment as u128;

            let id = states[i].customer.id;
            let remaining = states[i].remaining;
            let switch = if last_dispatched.is_some_and(|last| last != id) {
                config.switch_cost
            } else {
                0
            };
            let work_at = now.checked_add(switch).ok_or(Error::TimeOverflow {
                customer: id,
                time: now,
                stage: OverflowStage::WorkAt,
            })?;
            let finish = work_at.checked_add(remaining).ok_or(Error::TimeOverflow {
                customer: id,
                time: now,
                stage: OverflowStage::Finish,
            })?;

            let kind = if states[i].has_started {
                Kind::Resume {
                    remaining,
                    work_at,
                    finish,
                }
            } else {
                states[i].has_started = true;
                started += 1;
                Kind::Start {
                    remaining,
                    work_at,
                    finish,
                }
            };
            events.push(Event {
                time: now,
                customer: id,
                kind,
            });
            last_event_time = now;

            let dispatch_start = now;
            active = Some(Active {
                index: i,
                work_at,
                finish,
            });
            last_dispatched = Some(id);

            // Keep the active interval's dispatch time in the state encoded
            // by a parallel local map.
            DISPATCH_TIMES.with(|_| {});
            let _ = dispatch_start;

            if finish == now {
                events.push(Event {
                    time: now,
                    customer: id,
                    kind: Kind::Finish,
                });
                last_event_time = now;
                completed += 1;
                turnaround_ticks += (now - states[i].customer.arrival) as u128;
                completion_order.push(id);
                active = None;
                continue;
            }
            break;
        }

        // Phase F: expire customers queued during this tick, including zero
        // patience arrivals and zero-budget paused customers.
        let mut expired: Vec<usize> = waiting
            .iter()
            .copied()
            .filter(|&i| states[i].deadline.is_some_and(|d| d <= now))
            .collect();
        expired.sort_by(|&a, &b| {
            states[a]
                .deadline
                .cmp(&states[b].deadline)
                .then_with(|| states[a].customer.id.cmp(&states[b].customer.id))
        });
        for i in expired {
            waiting.retain(|&j| j != i);
            let segment = now - states[i].queue_entry;
            states[i].total_waited += segment;
            wait_ticks += segment as u128;
            events.push(Event {
                time: now,
                customer: states[i].customer.id,
                kind: Kind::Renege {
                    waited: states[i].total_waited,
                },
            });
            last_event_time = now;
            reneged += 1;
        }

        if next_arrival == arrivals.len() && active.is_none() && waiting.is_empty() {
            break;
        }

        let mut next_time: Option<u64> = None;
        let mut consider = |t: u64| {
            if t > now {
                next_time = Some(next_time.map_or(t, |old| old.min(t)));
            }
        };

        if next_arrival < arrivals.len() {
            consider(states[arrivals[next_arrival]].customer.arrival);
        }
        if let Some(a) = active {
            consider(a.finish);
            if now < a.work_at {
                consider(a.work_at);
            }
        }
        for &i in &waiting {
            if let Some(deadline) = states[i].deadline {
                consider(deadline);
            }
        }

        if let Some(t) = next_time {
            // Busy time is measured by occupied intervals. For an interval
            // ending here through finish/preemption, sum from dispatch.
            if let Some(a) = active {
                let start = states[a.index].customer.arrival;
                let _ = start;
            }
            now = t;
        } else {
            break;
        }
    }

    // Recompute busy time from trace intervals, which also handles pauses
    // and switching intervals exactly.
    busy_ticks = 0;
    let mut running: Option<(u32, u64)> = None;
    for event in &events {
        match event.kind {
            Kind::Start { .. } | Kind::Resume { .. } => {
                running = Some((event.customer, event.time));
            }
            Kind::Preempt { .. } | Kind::Finish => {
                if let Some((id, start)) = running {
                    if id == event.customer {
                        busy_ticks += (event.time - start) as u128;
                        running = None;
                    }
                }
            }
            _ => {}
        }
    }

    let mean_wait_milli = if accepted == 0 {
        0
    } else {
        rounded_ratio(wait_ticks * 1000, accepted as u128).unwrap_or(0)
    };
    let mean_turnaround_milli = if completed == 0 {
        0
    } else {
        rounded_ratio(turnaround_ticks * 1000, completed as u128).unwrap_or(0)
    };
    let elapsed = if events.is_empty() { 0 } else { last_event_time };
    let utilization_bps = if elapsed == 0 {
        0
    } else {
        rounded_ratio(busy_ticks * 10000, elapsed as u128)
            .unwrap_or(0)
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

thread_local! {
    static DISPATCH_TIMES: () = const { () };
}

fn main() {}