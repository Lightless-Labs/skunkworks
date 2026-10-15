use std::collections::HashSet;
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
    Preempt {
        remaining: u64,
        by: u32,
    },
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
#[derive(Clone, Copy)]
struct Active {
    index: usize,
    work_at: u64,
    finish: u64,
}
fn best(queue: &[usize], cs: &[Customer], paused: &[bool], ready: &[u64]) -> Option<usize> {
    queue.iter().copied().min_by(|&a, &b| {
        cs[a]
            .priority
            .cmp(&cs[b].priority)
            .then((!paused[a]).cmp(&(!paused[b])))
            .then(ready[a].cmp(&ready[b]))
            .then_with(|| {
                if paused[a] {
                    cs[a].id.cmp(&cs[b].id)
                } else {
                    cs[b].id.cmp(&cs[a].id)
                }
            })
    })
}
pub fn simulate(customers: &[Customer], switch_cost: u64) -> Result<Vec<Event>, Error> {
    let mut seen = HashSet::new();
    for (index, c) in customers.iter().enumerate() {
        if c.id == 0 {
            return Err(Error::ZeroId { index });
        }
        if !seen.insert(c.id) {
            return Err(Error::DuplicateId { index, id: c.id });
        }
        if c.priority > 3 {
            return Err(Error::BadPriority {
                index,
                value: c.priority,
            });
        }
    }
    let mut order: Vec<usize> = (0..customers.len()).collect();
    order.sort_by(|&a, &b| {
        customers[a]
            .arrival
            .cmp(&customers[b].arrival)
            .then(customers[a].priority.cmp(&customers[b].priority))
            .then(customers[b].id.cmp(&customers[a].id))
    });
    let mut remaining: Vec<u64> = customers.iter().map(|c| c.service).collect();
    let mut ready: Vec<u64> = customers.iter().map(|c| c.arrival).collect();
    let mut paused = vec![false; customers.len()];
    let mut started = vec![false; customers.len()];
    let mut queue = Vec::new();
    let mut active: Option<Active> = None;
    let mut previous: Option<usize> = None;
    let mut events = Vec::new();
    let mut next = 0;
    let mut time = 0;
    while next < order.len() || active.is_some() || !queue.is_empty() {
        time = order
            .get(next)
            .map(|&i| customers[i].arrival)
            .into_iter()
            .chain(active.map(|a| a.finish))
            .chain(active.filter(|a| a.work_at > time).map(|a| a.work_at))
            .min()
            .unwrap();
        if active.is_none() && queue.is_empty() {
            previous = None;
        }
        if active.is_some_and(|a| a.finish == time) {
            let a = active.take().unwrap();
            events.push(Event {
                time,
                customer: customers[a.index].id,
                kind: Kind::Finish,
            });
        }
        while next < order.len() && customers[order[next]].arrival == time {
            let i = order[next];
            next += 1;
            queue.push(i);
            events.push(Event {
                time,
                customer: customers[i].id,
                kind: Kind::Arrive,
            });
        }
        if let (Some(a), Some(b)) = (active, best(&queue, customers, &paused, &ready)) {
            if time >= a.work_at && customers[b].priority < customers[a.index].priority {
                let i = a.index;
                remaining[i] = a.finish - time;
                paused[i] = true;
                ready[i] = time;
                queue.push(i);
                events.push(Event {
                    time,
                    customer: customers[i].id,
                    kind: Kind::Preempt {
                        remaining: remaining[i],
                        by: customers[b].id,
                    },
                });
                active = None;
            }
        }
        while active.is_none() {
            let Some(i) = best(&queue, customers, &paused, &ready) else {
                break;
            };
            queue.retain(|&x| x != i);
            let cost = if previous.is_some_and(|p| p != i) {
                switch_cost
            } else {
                0
            };
            let work_at = time.checked_add(cost).ok_or(Error::TimeOverflow {
                customer: customers[i].id,
                time,
            })?;
            let finish = work_at
                .checked_add(remaining[i])
                .ok_or(Error::TimeOverflow {
                    customer: customers[i].id,
                    time,
                })?;
            let kind = if started[i] {
                Kind::Resume {
                    remaining: remaining[i],
                    work_at,
                    finish,
                }
            } else {
                Kind::Start {
                    remaining: remaining[i],
                    work_at,
                    finish,
                }
            };
            events.push(Event {
                time,
                customer: customers[i].id,
                kind,
            });
            started[i] = true;
            previous = Some(i);
            if finish == time {
                events.push(Event {
                    time,
                    customer: customers[i].id,
                    kind: Kind::Finish,
                });
            } else {
                active = Some(Active {
                    index: i,
                    work_at,
                    finish,
                });
            }
        }
    }
    Ok(events)
}
