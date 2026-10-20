#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Customer {
    pub id: u32,
    pub arrival: u64,
    pub service: u64,
    pub priority: u8,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Kind {
    Arrive,
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
pub enum Error {
    ZeroId { index: usize },
    DuplicateId { index: usize, id: u32 },
    BadPriority { index: usize, value: u8 },
    TimeOverflow { customer: u32, time: u64 },
}

#[derive(Clone)]
struct Job {
    c: Customer,
    remaining: u64,
    pause: Option<u64>,
}

#[derive(Clone)]
struct Active {
    c: Customer,
    work_at: u64,
    finish: u64,
}

fn rank(a: &Job, b: &Job) -> std::cmp::Ordering {
    use std::cmp::Ordering;
    a.c.priority
        .cmp(&b.c.priority)
        .then_with(|| match (a.pause, b.pause) {
            (Some(x), Some(y)) => x.cmp(&y).then_with(|| a.c.id.cmp(&b.c.id)),
            (Some(_), None) => Ordering::Less,
            (None, Some(_)) => Ordering::Greater,
            (None, None) => a
                .c
                .arrival
                .cmp(&b.c.arrival)
                .then_with(|| b.c.id.cmp(&a.c.id)),
        })
}

pub fn simulate(customers: &[Customer], switch_cost: u64) -> Result<Vec<Event>, Error> {
    let mut seen = std::collections::HashSet::new();
    for (index, c) in customers.iter().enumerate() {
        if c.id == 0 {
            return Err(Error::ZeroId { index });
        }
        if !seen.insert(c.id) {
            return Err(Error::DuplicateId { index, id: c.id });
        }
        if c.priority > 3 {
            return Err(Error::BadPriority { index, value: c.priority });
        }
    }
    if customers.is_empty() {
        return Ok(vec![]);
    }

    let mut arrivals = customers.to_vec();
    arrivals.sort_by(|a, b| {
        a.arrival.cmp(&b.arrival)
            .then(a.priority.cmp(&b.priority))
            .then(b.id.cmp(&a.id))
    });

    let mut trace = Vec::new();
    let mut queue: Vec<Job> = Vec::new();
    let mut active: Option<Active> = None;
    let mut ai = 0;
    let mut done = 0;
    let mut previous: Option<u32> = None;
    let mut ever_started = std::collections::HashSet::new();
    let mut clock = 0u64;
    let mut has_clock = false;

    while done < customers.len() {
        let mut next = arrivals.get(ai).map(|c| c.arrival);
        if let Some(a) = &active {
            next = Some(next.map_or(a.finish, |x| x.min(a.finish)));
            if a.work_at > clock || !has_clock {
                next = Some(next.map_or(a.work_at, |x| x.min(a.work_at)));
            }
        }
        let t = next.expect("some customer is unfinished");
        clock = t;
        has_clock = true;

        if active.as_ref().is_some_and(|a| a.finish == t) {
            let a = active.take().unwrap();
            trace.push(Event { time: t, customer: a.c.id, kind: Kind::Finish });
            done += 1;
        }

        while ai < arrivals.len() && arrivals[ai].arrival == t {
            let c = arrivals[ai].clone();
            trace.push(Event { time: t, customer: c.id, kind: Kind::Arrive });
            queue.push(Job { remaining: c.service, c, pause: None });
            ai += 1;
        }

        if let Some(a) = active.as_ref() {
            if t >= a.work_at {
                if let Some(i) = queue.iter().enumerate().min_by(|(_, x), (_, y)| rank(x, y)).map(|(i, _)| i) {
                    if queue[i].c.priority < a.c.priority {
                        let best_id = queue[i].c.id;
                        let a = active.take().unwrap();
                        let rem = a.finish - t;
                        trace.push(Event {
                            time: t,
                            customer: a.c.id,
                            kind: Kind::Preempt { remaining: rem, by: best_id },
                        });
                        queue.push(Job { c: a.c, remaining: rem, pause: Some(t) });
                    }
                }
            }
        }

        while active.is_none() && !queue.is_empty() {
            let i = queue.iter().enumerate().min_by(|(_, x), (_, y)| rank(x, y)).map(|(i, _)| i).unwrap();
            let job = queue.remove(i);
            let cost = match previous {
                None => 0,
                Some(id) if id == job.c.id => 0,
                Some(_) => switch_cost,
            };
            let work_at = t.checked_add(cost).ok_or(Error::TimeOverflow { customer: job.c.id, time: t })?;
            let finish = work_at.checked_add(job.remaining).ok_or(Error::TimeOverflow { customer: job.c.id, time: t })?;

            let kind = if ever_started.insert(job.c.id) {
                Kind::Start { remaining: job.remaining, work_at, finish }
            } else {
                Kind::Resume { remaining: job.remaining, work_at, finish }
            };
            trace.push(Event { time: t, customer: job.c.id, kind });
            previous = Some(job.c.id);

            if finish == t {
                trace.push(Event { time: t, customer: job.c.id, kind: Kind::Finish });
                done += 1;
            } else {
                active = Some(Active { c: job.c, work_at, finish });
            }
        }
    }

    Ok(trace)
}
