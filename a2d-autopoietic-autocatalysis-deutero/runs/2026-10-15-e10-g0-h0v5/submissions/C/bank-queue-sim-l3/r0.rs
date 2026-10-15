use std::cmp::Ordering;
use std::collections::{BinaryHeap, HashSet};

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
struct Waiting {
    customer: Customer,
    remaining: u64,
    paused_at: Option<u64>,
}

impl Waiting {
    fn precedes(&self, other: &Self) -> bool {
        if self.customer.priority != other.customer.priority {
            return self.customer.priority < other.customer.priority;
        }

        match (self.paused_at, other.paused_at) {
            (Some(a), Some(b)) => {
                a < b || (a == b && self.customer.id < other.customer.id)
            }
            (Some(_), None) => true,
            (None, Some(_)) => false,
            (None, None) => {
                self.customer.arrival < other.customer.arrival
                    || (self.customer.arrival == other.customer.arrival
                        && self.customer.id > other.customer.id)
            }
        }
    }
}

impl PartialEq for Waiting {
    fn eq(&self, other: &Self) -> bool {
        self.customer.id == other.customer.id
    }
}
impl Eq for Waiting {}

impl PartialOrd for Waiting {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}
impl Ord for Waiting {
    fn cmp(&self, other: &Self) -> Ordering {
        if self.precedes(other) {
            Ordering::Greater
        } else if other.precedes(self) {
            Ordering::Less
        } else {
            Ordering::Equal
        }
    }
}

struct Running {
    customer: Customer,
    finish: u64,
    work_at: u64,
}

pub fn simulate(customers: &[Customer], switch_cost: u64) -> Result<Vec<Event>, Error> {
    let mut ids = HashSet::with_capacity(customers.len());
    for (index, customer) in customers.iter().enumerate() {
        if customer.id == 0 {
            return Err(Error::ZeroId { index });
        }
        if !ids.insert(customer.id) {
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
    }

    if customers.is_empty() {
        return Ok(Vec::new());
    }

    let mut arrivals = customers.to_vec();
    arrivals.sort_by(|a, b| {
        a.arrival
            .cmp(&b.arrival)
            .then(a.priority.cmp(&b.priority))
            .then(b.id.cmp(&a.id))
    });

    let mut trace = Vec::new();
    let mut waiting = BinaryHeap::<Waiting>::new();
    let mut running: Option<Running> = None;
    let mut next_arrival = 0usize;
    let mut last_dispatched: Option<u32> = None;

    loop {
        if next_arrival == arrivals.len() && running.is_none() && waiting.is_empty() {
            break;
        }

        let next_arrival_time = arrivals.get(next_arrival).map(|c| c.arrival);
        let next_time = match (next_arrival_time, running.as_ref()) {
            (Some(arrival), Some(active)) => {
                let mut next = arrival.min(active.finish);
                if active.work_at > active.finish {
                    next = next.min(active.work_at);
                } else if active.work_at >= 0 {
                    next = next.min(active.work_at);
                }
                Some(next)
            }
            (Some(arrival), None) => Some(arrival),
            (None, Some(active)) => Some(active.finish),
            (None, None) => None,
        };

        let Some(time) = next_time else {
            break;
        };

        if running.as_ref().is_some_and(|active| active.finish == time) {
            let done = running.take().unwrap();
            trace.push(Event {
                time,
                customer: done.customer.id,
                kind: Kind::Finish,
            });
        }

        while next_arrival < arrivals.len() && arrivals[next_arrival].arrival == time {
            let customer = arrivals[next_arrival].clone();
            trace.push(Event {
                time,
                customer: customer.id,
                kind: Kind::Arrive,
            });
            waiting.push(Waiting {
                remaining: customer.service,
                customer,
                paused_at: None,
            });
            next_arrival += 1;
        }

        if let Some(active) = running.as_ref() {
            if time >= active.work_at && time < active.finish {
                if let Some(best) = waiting.peek() {
                    if best.customer.priority < active.customer.priority {
                        let by = best.customer.id;
                        let active = running.take().unwrap();
                        let remaining = active.finish - time;
                        trace.push(Event {
                            time,
                            customer: active.customer.id,
                            kind: Kind::Preempt { remaining, by },
                        });
                        waiting.push(Waiting {
                            customer: active.customer,
                            remaining,
                            paused_at: Some(time),
                        });
                    }
                }
            }
        }

        while running.is_none() {
            let Some(item) = waiting.pop() else {
                break;
            };
            let cost = if last_dispatched.is_some_and(|id| id != item.customer.id) {
                switch_cost
            } else {
                0
            };
            let work_at = time.checked_add(cost).ok_or(Error::TimeOverflow {
                customer: item.customer.id,
                time,
            })?;
            let finish = work_at.checked_add(item.remaining).ok_or(Error::TimeOverflow {
                customer: item.customer.id,
                time,
            })?;

            let kind = if item.paused_at.is_some() {
                Kind::Resume {
                    remaining: item.remaining,
                    work_at,
                    finish,
                }
            } else {
                Kind::Start {
                    remaining: item.remaining,
                    work_at,
                    finish,
                }
            };
            trace.push(Event {
                time,
                customer: item.customer.id,
                kind,
            });
            last_dispatched = Some(item.customer.id);
            running = Some(Running {
                customer: item.customer,
                finish,
                work_at,
            });

            if finish == time {
                let done = running.take().unwrap();
                trace.push(Event {
                    time,
                    customer: done.customer.id,
                    kind: Kind::Finish,
                });
            } else {
                break;
            }
        }
    }

    Ok(trace)
}

fn main() {}