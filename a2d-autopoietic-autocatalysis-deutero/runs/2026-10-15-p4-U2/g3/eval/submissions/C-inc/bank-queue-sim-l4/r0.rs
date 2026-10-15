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
    let complement = denominator - r;
    Some(if r > complement || (r == complement && q % 2 == 1) {
        q + 1
    } else {
        q
    })
}

#[derive(Clone)]
struct CustomerState {
    customer: Customer,
    initial_deadline: Option<u64>,
    waiting: bool,
    queue_entry: u64,
    deadline: Option<u64>,
    remaining: u64,
    waited: u128,
    started: bool,
    pause_time: Option<u64>,
}

#[derive(Clone)]
struct Active {
    index: usize,
    work_at: u64,
    finish: u64,
    protected: bool,
    interval_start: u64,
}

fn push_event(events: &mut Vec<Event>, time: u64, customer: u32, kind: Kind) {
    events.push(Event { time, customer, kind });
}

fn better_waiter(a: usize, b: usize, states: &[CustomerState]) -> bool {
    let x = &states[a];
    let y = &states[b];
    if x.customer.priority != y.customer.priority {
        return x.customer.priority < y.customer.priority;
    }
    match (x.pause_time, y.pause_time) {
        (Some(px), Some(py)) => {
            px < py || (px == py && x.customer.id < y.customer.id)
        }
        (Some(_), None) => true,
        (None, Some(_)) => false,
        (None, None) => {
            x.customer.arrival < y.customer.arrival
                || (x.customer.arrival == y.customer.arrival
                    && x.customer.id > y.customer.id)
        }
    }
}

fn best_waiter(states: &[CustomerState]) -> Option<usize> {
    let mut best = None;
    for (i, state) in states.iter().enumerate() {
        if state.waiting && best.map_or(true, |b| better_waiter(i, b, states)) {
            best = Some(i);
        }
    }
    best
}

fn expire_waiters(
    states: &mut [CustomerState],
    now: u64,
    events: &mut Vec<Event>,
    stats: &mut Stats,
) {
    let mut expired: Vec<usize> = states
        .iter()
        .enumerate()
        .filter_map(|(i, s)| {
            if s.waiting && s.deadline.is_some_and(|d| d <= now) {
                Some(i)
            } else {
                None
            }
        })
        .collect();
    expired.sort_by_key(|&i| (states[i].deadline.unwrap(), states[i].customer.id));

    for i in expired {
        let s = &mut states[i];
        let segment = now - s.queue_entry;
        s.waited += segment as u128;
        stats.wait_ticks += segment as u128;
        s.waiting = false;
        stats.reneged += 1;
        push_event(
            events,
            now,
            s.customer.id,
            Kind::Renege {
                waited: s.waited as u64,
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

    let mut ids = Vec::with_capacity(customers.len());
    let mut initial_deadlines = Vec::with_capacity(customers.len());
    for (index, c) in customers.iter().enumerate() {
        if c.id == 0 {
            return Err(Error::ZeroId { index });
        }
        if ids.contains(&c.id) {
            return Err(Error::DuplicateId { index, id: c.id });
        }
        ids.push(c.id);
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
        initial_deadlines.push(deadline);
    }

    let mut states: Vec<CustomerState> = customers
        .iter()
        .cloned()
        .zip(initial_deadlines)
        .map(|(customer, initial_deadline)| CustomerState {
            remaining: customer.service,
            customer,
            initial_deadline,
            waiting: false,
            queue_entry: 0,
            deadline: None,
            waited: 0,
            started: false,
            pause_time: None,
        })
        .collect();

    let mut arrivals: Vec<usize> = (0..states.len()).collect();
    arrivals.sort_by(|&a, &b| {
        states[a]
            .customer
            .arrival
            .cmp(&states[b].customer.arrival)
            .then_with(|| states[a].customer.priority.cmp(&states[b].customer.priority))
            .then_with(|| states[b].customer.id.cmp(&states[a].customer.id))
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

    let mut now = 0u64;
    let mut next_arrival = 0usize;
    let mut active: Option<Active> = None;
    let mut last_dispatched: Option<u32> = None;

    while stats.completed + stats.reneged + stats.balked < customers.len() {
        if active.is_none() && best_waiter(&states).is_none() && next_arrival < arrivals.len() {
            now = now.max(states[arrivals[next_arrival]].customer.arrival);
        }

        if let Some(a) = active.as_ref() {
            now = now.max(a.work_at.min(a.finish));
        }

        if active.as_ref().is_some_and(|a| a.finish <= now) {
            let a = active.take().unwrap();
            stats.busy_ticks += (now - a.interval_start) as u128;
            let i = a.index;
            stats.completed += 1;
            stats.turnaround_ticks += (now - states[i].customer.arrival) as u128;
            stats.completion_order.push(states[i].customer.id);
            push_event(&mut events, now, states[i].customer.id, Kind::Finish);
        }

        expire_waiters(&mut states, now, &mut events, &mut stats);

        if next_arrival < arrivals.len()
            && states[arrivals[next_arrival]].customer.arrival <= now
        {
            let cohort_time = states[arrivals[next_arrival]].customer.arrival;
            while next_arrival < arrivals.len()
                && states[arrivals[next_arrival]].customer.arrival == cohort_time
            {
                let i = arrivals[next_arrival];
                next_arrival += 1;
                let id = states[i].customer.id;
                push_event(&mut events, now, id, Kind::Arrive);

                let waiting_count = states.iter().filter(|s| s.waiting).count();
                let limit = config.waiting_capacity + usize::from(active.is_none());
                if waiting_count >= limit {
                    stats.balked += 1;
                    push_event(&mut events, now, id, Kind::Balk);
                } else {
                    let s = &mut states[i];
                    s.waiting = true;
                    s.queue_entry = now;
                    s.deadline = s.initial_deadline;
                    stats.accepted += 1;
                }
            }
        }

        if let Some(a) = active.as_mut() {
            if now >= a.work_at {
                a.protected = false;
            }
        }

        if let Some(a) = active.as_ref() {
            if !a.protected {
                if let Some(w) = best_waiter(&states) {
                    if states[w].customer.priority < states[a.index].customer.priority {
                        let i = a.index;
                        let remaining = a.finish - now;
                        let id = states[i].customer.id;
                        let by = states[w].customer.id;
                        let deadline = match states[i].customer.patience {
                            Some(budget) => Some(now.checked_add(budget).ok_or(
                                Error::TimeOverflow {
                                    customer: id,
                                    time: now,
                                    stage: OverflowStage::Patience,
                                },
                            )?),
                            None => None,
                        };

                        push_event(
                            &mut events,
                            now,
                            id,
                            Kind::Preempt { remaining, by },
                        );
                        stats.preemptions += 1;
                        let old = active.take().unwrap();
                        stats.busy_ticks += (now - old.interval_start) as u128;

                        let s = &mut states[i];
                        s.remaining = remaining;
                        s.waiting = true;
                        s.queue_entry = now;
                        s.deadline = deadline;
                        s.pause_time = Some(now);
                    }
                }
            }
        }

        while active.is_none() {
            let Some(i) = best_waiter(&states) else {
                break;
            };
            let id = states[i].customer.id;
            let remaining = states[i].remaining;
            let first = !states[i].started;
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

            let segment = now - states[i].queue_entry;
            states[i].waited += segment as u128;
            stats.wait_ticks += segment as u128;
            states[i].waiting = false;
            states[i].deadline = None;
            states[i].pause_time = None;
            if first {
                states[i].started = true;
                stats.started += 1;
            }

            last_dispatched = Some(id);
            push_event(
                &mut events,
                now,
                id,
                if first {
                    Kind::Start {
                        remaining,
                        work_at,
                        finish,
                    }
                } else {
                    Kind::Resume {
                        remaining,
                        work_at,
                        finish,
                    }
                },
            );

            active = Some(Active {
                index: i,
                work_at,
                finish,
                protected: switch > 0,
                interval_start: now,
            });

            if finish == now {
                active = None;
                stats.busy_ticks += (finish - now) as u128;
                stats.completed += 1;
                stats.turnaround_ticks += (now - states[i].customer.arrival) as u128;
                stats.completion_order.push(id);
                push_event(&mut events, now, id, Kind::Finish);
                continue;
            }
            break;
        }

        expire_waiters(&mut states, now, &mut events, &mut stats);

        if active.is_none()
            && best_waiter(&states).is_none()
            && next_arrival >= arrivals.len()
        {
            break;
        }

        let mut next: Option<u64> = None;
        if next_arrival < arrivals.len() {
            next = Some(states[arrivals[next_arrival]].customer.arrival);
        }
        if let Some(a) = &active {
            if a.work_at > now {
                next = Some(next.map_or(a.work_at, |n| n.min(a.work_at)));
            }
            if a.finish > now {
                next = Some(next.map_or(a.finish, |n| n.min(a.finish)));
            }
        }
        for s in &states {
            if s.waiting {
                if let Some(d) = s.deadline.filter(|&d| d > now) {
                    next = Some(next.map_or(d, |n| n.min(d)));
                }
            }
        }
        if let Some(next_time) = next {
            now = next_time;
        } else {
            break;
        }
    }

    stats.elapsed = events.last().map_or(0, |e| e.time);
    stats.mean_wait_milli =
        rounded_ratio(stats.wait_ticks * 1000, stats.accepted as u128).unwrap_or(0);
    stats.mean_turnaround_milli =
        rounded_ratio(stats.turnaround_ticks * 1000, stats.completed as u128).unwrap_or(0);
    stats.utilization_bps = rounded_ratio(stats.busy_ticks * 10000, stats.elapsed as u128)
        .unwrap_or(0) as u32;

    Ok(Report { events, stats })
}

fn main() {}