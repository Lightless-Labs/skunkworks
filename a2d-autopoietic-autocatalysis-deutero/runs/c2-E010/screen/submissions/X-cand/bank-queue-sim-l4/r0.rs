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
    Arrive, Balk, Renege { waited: u64 },
    Start { remaining: u64, work_at: u64, finish: u64 },
    Resume { remaining: u64, work_at: u64, finish: u64 },
    Preempt { remaining: u64, by: u32 }, Finish
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Event { pub time: u64, pub customer: u32, pub kind: Kind }
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum OverflowStage { WorkAt, Finish, Patience }
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Error {
    CapacityTooLarge { capacity: usize }, TooManyCustomers { count: usize },
    ZeroId { index: usize }, DuplicateId { index: usize, id: u32 },
    BadPriority { index: usize, value: u8 }, DeadlineOverflow { index: usize, id: u32 },
    TimeOverflow { customer: u32, time: u64, stage: OverflowStage }
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Stats {
    pub accepted: usize, pub balked: usize, pub reneged: usize, pub completed: usize,
    pub started: usize, pub preemptions: usize, pub elapsed: u64,
    pub busy_ticks: u128, pub wait_ticks: u128, pub turnaround_ticks: u128,
    pub mean_wait_milli: u128, pub mean_turnaround_milli: u128, pub utilization_bps: u32,
    pub completion_order: Vec<u32>
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Report { pub events: Vec<Event>, pub stats: Stats }

pub fn rounded_ratio(numerator: u128, denominator: u128) -> Option<u128> {
    if denominator == 0 { return None; }
    let q = numerator / denominator;
    let r = numerator % denominator;
    let other = denominator - r;
    Some(if r > other || (r == other && q & 1 == 1) { q + 1 } else { q })
}

#[derive(Clone)]
struct Person {
    c: Customer,
    deadline: Option<u64>,
    remaining: u64,
    waited: u64,
    entry: u64,
    pause_time: Option<u64>,
    started: bool,
    done: bool,
}
#[derive(Clone)]
struct Active { idx: usize, work_at: u64, finish: u64, protected: bool }

fn emit(events: &mut Vec<Event>, t: u64, id: u32, kind: Kind) {
    events.push(Event { time: t, customer: id, kind });
}
fn order_key(p: &Person) -> (u8, u8, u64, u32) {
    match p.pause_time {
        Some(t) => (p.c.priority, 0, t, p.c.id),
        None => (p.c.priority, 1, p.c.arrival, u32::MAX - p.c.id),
    }
}
fn best(queue: &[usize], people: &[Person]) -> Option<usize> {
    queue.iter().copied().min_by_key(|&i| order_key(&people[i]))
}

pub fn simulate(customers: &[Customer], config: &Config) -> Result<Report, Error> {
    if config.waiting_capacity > 4096 {
        return Err(Error::CapacityTooLarge { capacity: config.waiting_capacity });
    }
    if customers.len() > 4096 { return Err(Error::TooManyCustomers { count: customers.len() }); }

    let mut ids = Vec::with_capacity(customers.len());
    for (i, c) in customers.iter().enumerate() {
        if c.id == 0 { return Err(Error::ZeroId { index: i }); }
        if ids.contains(&c.id) { return Err(Error::DuplicateId { index: i, id: c.id }); }
        if c.priority > 3 { return Err(Error::BadPriority { index: i, value: c.priority }); }
        if let Some(p) = c.patience {
            if c.arrival.checked_add(p).is_none() {
                return Err(Error::DeadlineOverflow { index: i, id: c.id });
            }
        }
        ids.push(c.id);
    }

    let mut people: Vec<Person> = customers.iter().cloned().map(|c| Person {
        deadline: c.patience.map(|p| c.arrival + p),
        remaining: c.service, waited: 0, entry: c.arrival, pause_time: None,
        started: false, done: false, c,
    }).collect();
    let mut arrivals: Vec<usize> = (0..people.len()).collect();
    arrivals.sort_by(|&a, &b| {
        people[a].c.arrival.cmp(&people[b].c.arrival)
            .then(people[a].c.priority.cmp(&people[b].c.priority))
            .then(people[b].c.id.cmp(&people[a].c.id))
    });

    let mut events = Vec::new();
    let mut queue: Vec<usize> = Vec::new();
    let mut active: Option<Active> = None;
    let mut ai = 0usize;
    let mut last_dispatch: Option<usize> = None;
    let mut stats = Stats {
        accepted: 0, balked: 0, reneged: 0, completed: 0, started: 0, preemptions: 0,
        elapsed: 0, busy_ticks: 0, wait_ticks: 0, turnaround_ticks: 0,
        mean_wait_milli: 0, mean_turnaround_milli: 0, utilization_bps: 0,
        completion_order: Vec::new(),
    };
    let mut now = 0u64;
    let mut last_event = 0u64;
    let mut has_event = false;

    while stats.completed + stats.reneged + stats.balked < people.len() {
        let next_arrival = arrivals.get(ai).map(|&i| people[i].c.arrival);
        let next_finish = active.as_ref().map(|a| a.finish);
        let next_work = active.as_ref().filter(|a| a.protected).map(|a| a.work_at);
        let next_deadline = queue.iter().filter_map(|&i| people[i].deadline).min();
        let t = [next_arrival, next_finish, next_work, next_deadline]
            .into_iter().flatten().min().unwrap();
        now = t;

        // A: completion.
        if active.as_ref().is_some_and(|a| a.finish == t) {
            let a = active.take().unwrap();
            let p = &mut people[a.idx];
            emit(&mut events, t, p.c.id, Kind::Finish);
            p.done = true;
            stats.completed += 1;
            stats.turnaround_ticks += (t - p.c.arrival) as u128;
            stats.completion_order.push(p.c.id);
            last_event = t; has_event = true;
        }

        // B: old waiters expire.
        let mut exp: Vec<usize> = queue.iter().copied()
            .filter(|&i| people[i].deadline.is_some_and(|d| d <= t)).collect();
        exp.sort_by_key(|&i| (people[i].deadline.unwrap(), people[i].c.id));
        for i in exp {
            if let Some(pos) = queue.iter().position(|&x| x == i) {
                queue.remove(pos);
                let p = &mut people[i];
                let seg = t - p.entry;
                p.waited += seg;
                stats.wait_ticks += seg as u128;
                emit(&mut events, t, p.c.id, Kind::Renege { waited: p.waited });
                p.done = true;
                stats.reneged += 1;
                last_event = t; has_event = true;
            }
        }

        // C: arrivals.
        while ai < arrivals.len() && people[arrivals[ai]].c.arrival == t {
            let i = arrivals[ai]; ai += 1;
            let id = people[i].c.id;
            emit(&mut events, t, id, Kind::Arrive);
            last_event = t; has_event = true;
            let slots = config.waiting_capacity + usize::from(active.is_none());
            if queue.len() >= slots {
                emit(&mut events, t, id, Kind::Balk);
                stats.balked += 1;
                last_event = t;
            } else {
                stats.accepted += 1;
                people[i].entry = t;
                queue.push(i);
            }
        }

        // D: preemption after protection.
        let can_preempt = active.as_ref().is_some_and(|a| !a.protected && a.work_at <= t);
        if can_preempt {
            if let (Some(bi), Some(a)) = (best(&queue, &people), active.as_ref()) {
                if people[bi].c.priority < people[a.idx].c.priority {
                    let old = active.take().unwrap();
                    let rem = old.finish - t;
                    let id = people[old.idx].c.id;
                    let by = people[bi].c.id;
                    emit(&mut events, t, id, Kind::Preempt { remaining: rem, by });
                    stats.preemptions += 1;
                    stats.busy_ticks += (t - old_start(&old, t)) as u128;
                    // Correct occupied interval start is retained separately below.
                    let p = &mut people[old.idx];
                    p.remaining = rem;
                    p.entry = t;
                    p.pause_time = Some(t);
                    if let Some(budget) = p.c.patience {
                        let spent = p.waited;
                        let remaining_budget = budget.saturating_sub(spent);
                        p.deadline = Some(t.checked_add(remaining_budget).ok_or(Error::TimeOverflow {
                            customer: p.c.id, time: t, stage: OverflowStage::Patience
                        })?);
                    }
                    queue.push(old.idx);
                    active = None;
                    last_event = t; has_event = true;
                }
            }
        }

        // E: dispatch and inline completions.
        loop {
            if active.is_some() { break; }
            let Some(i) = best(&queue, &people) else { break; };
            queue.retain(|&x| x != i);
            let p = &mut people[i];
            let seg = t - p.entry;
            p.waited += seg;
            stats.wait_ticks += seg as u128;
            let was_started = p.started;
            let cost = if last_dispatch.is_some_and(|x| x != i) { config.switch_cost } else { 0 };
            let work_at = t.checked_add(cost).ok_or(Error::TimeOverflow {
                customer: p.c.id, time: t, stage: OverflowStage::WorkAt
            })?;
            let finish = work_at.checked_add(p.remaining).ok_or(Error::TimeOverflow {
                customer: p.c.id, time: t, stage: OverflowStage::Finish
            })?;
            if was_started {
                emit(&mut events, t, p.c.id, Kind::Resume { remaining: p.remaining, work_at, finish });
            } else {
                emit(&mut events, t, p.c.id, Kind::Start { remaining: p.remaining, work_at, finish });
                p.started = true;
                stats.started += 1;
            }
            last_event = t; has_event = true;
            last_dispatch = Some(i);
            active = Some(Active { idx: i, work_at, finish, protected: work_at > t });
            if finish == t {
                active = None;
                p.done = true;
                stats.completed += 1;
                stats.turnaround_ticks += (t - p.c.arrival) as u128;
                stats.completion_order.push(p.c.id);
                emit(&mut events, t, p.c.id, Kind::Finish);
                last_event = t;
                continue;
            }
            break;
        }

        // F: expire any newly queued waiters.
        let mut exp: Vec<usize> = queue.iter().copied()
            .filter(|&i| people[i].deadline.is_some_and(|d| d <= t)).collect();
        exp.sort_by_key(|&i| (people[i].deadline.unwrap(), people[i].c.id));
        for i in exp {
            if let Some(pos) = queue.iter().position(|&x| x == i) {
                queue.remove(pos);
                let p = &mut people[i];
                let seg = t - p.entry;
                p.waited += seg;
                stats.wait_ticks += seg as u128;
                emit(&mut events, t, p.c.id, Kind::Renege { waited: p.waited });
                p.done = true;
                stats.reneged += 1;
                last_event = t; has_event = true;
            }
        }

        // Mark protection boundary as ended after all phases at this tick.
        if let Some(a) = active.as_mut() {
            if a.work_at == t { a.protected = false; }
        }

        // Track active interval start via a side table is not needed for correct simulation;
        // busy ticks are accrued at finish/preemption in a separate pass below.
    }

    // Compute occupied intervals from the trace: dispatch to next preempt or finish.
    let mut open: Option<(u32, u64)> = None;
    stats.busy_ticks = 0;
    for ev in &events {
        match ev.kind {
            Kind::Start { .. } | Kind::Resume { .. } => open = Some((ev.customer, ev.time)),
            Kind::Preempt { .. } | Kind::Finish => {
                if let Some((_, s)) = open.take() {
                    stats.busy_ticks += (ev.time - s) as u128;
                }
            }
            _ => {}
        }
    }
    stats.elapsed = if has_event { last_event } else { 0 };
    let accepted = stats.accepted as u128;
    stats.mean_wait_milli = if accepted == 0 { 0 } else {
        rounded_ratio(stats.wait_ticks * 1000, accepted).unwrap()
    };
    stats.mean_turnaround_milli = if stats.completed == 0 { 0 } else {
        rounded_ratio(stats.turnaround_ticks * 1000, stats.completed as u128).unwrap()
    };
    stats.utilization_bps = if stats.elapsed == 0 { 0 } else {
        rounded_ratio(stats.busy_ticks * 10000, stats.elapsed as u128).unwrap().min(u32::MAX as u128) as u32
    };
    Ok(Report { events, stats })
}
fn old_start(_a: &Active, now: u64) -> u64 { now }
