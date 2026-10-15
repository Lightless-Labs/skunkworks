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
    let half = denominator / 2;
    let round_up = if denominator % 2 == 0 {
        r > half || (r == half && q % 2 == 1)
    } else {
        r > half
    };
    Some(if round_up { q + 1 } else { q })
}

#[derive(Clone)]
struct Person {
    id: u32,
    arrival: u64,
    service: u64,
    priority: u8,
    patience: Option<u64>,
    deadline: Option<u64>,
    remaining: u64,
    waited: u64,
    entry: u64,
    paused: bool,
    pause_time: u64,
    started: bool,
}

#[derive(Clone)]
struct Active {
    idx: usize,
    work_at: u64,
    finish: u64,
    dispatch_time: u64,
}

fn waiter_better(a: &Person, b: &Person) -> bool {
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
        a.id < b.id
    } else if a.arrival != b.arrival {
        a.arrival < b.arrival
    } else {
        a.id > b.id
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

    let mut seen = Vec::<u32>::new();
    let mut people = Vec::<Person>::with_capacity(customers.len());

    for (index, c) in customers.iter().enumerate() {
        if c.id == 0 {
            return Err(Error::ZeroId { index });
        }
        if seen.contains(&c.id) {
            return Err(Error::DuplicateId { index, id: c.id });
        }
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
        seen.push(c.id);
        people.push(Person {
            id: c.id,
            arrival: c.arrival,
            service: c.service,
            priority: c.priority,
            patience: c.patience,
            deadline,
            remaining: c.service,
            waited: 0,
            entry: c.arrival,
            paused: false,
            pause_time: 0,
            started: false,
        });
    }

    let mut arrival_order: Vec<usize> = (0..people.len()).collect();
    arrival_order.sort_by(|&a, &b| {
        people[a]
            .arrival
            .cmp(&people[b].arrival)
            .then(people[a].priority.cmp(&people[b].priority))
            .then(people[b].id.cmp(&people[a].id))
    });

    let mut events = Vec::<Event>::new();
    let mut queue = Vec::<usize>::new();
    let mut active: Option<Active> = None;
    let mut next_arrival = 0usize;
    let mut now = 0u64;
    let mut last_dispatch: Option<u32> = None;
    let mut accepted = 0usize;
    let mut balked = 0usize;
    let mut reneged = 0usize;
    let mut completed = 0usize;
    let mut started = 0usize;
    let mut preemptions = 0usize;
    let mut busy_ticks = 0u128;
    let mut wait_ticks = 0u128;
    let mut turnaround_ticks = 0u128;
    let mut completion_order = Vec::<u32>::new();

    while next_arrival < arrival_order.len() || active.is_some() || !queue.is_empty() {
        if active.is_none() && queue.is_empty() {
            if next_arrival >= arrival_order.len() {
                break;
            }
            now = now.max(people[arrival_order[next_arrival]].arrival);
        }

        if active.as_ref().is_some_and(|a| a.finish == now) {
            let a = active.take().unwrap();
            let idx = a.idx;
            busy_ticks += (a.finish - a.dispatch_time) as u128;
            events.push(Event {
                time: now,
                customer: people[idx].id,
                kind: Kind::Finish,
            });
            completed += 1;
            turnaround_ticks += (now - people[idx].arrival) as u128;
            completion_order.push(people[idx].id);
        }

        let mut expired: Vec<usize> = queue
            .iter()
            .copied()
            .filter(|&i| people[i].deadline.is_some_and(|d| d <= now))
            .collect();
        expired.sort_by(|&a, &b| {
            people[a]
                .deadline
                .cmp(&people[b].deadline)
                .then(people[a].id.cmp(&people[b].id))
        });
        for idx in expired {
            queue.retain(|&x| x != idx);
            let segment = now - people[idx].entry;
            people[idx].waited += segment;
            wait_ticks += segment as u128;
            events.push(Event {
                time: now,
                customer: people[idx].id,
                kind: Kind::Renege {
                    waited: people[idx].waited,
                },
            });
            reneged += 1;
        }

        while next_arrival < arrival_order.len()
            && people[arrival_order[next_arrival]].arrival == now
        {
            let idx = arrival_order[next_arrival];
            next_arrival += 1;
            events.push(Event {
                time: now,
                customer: people[idx].id,
                kind: Kind::Arrive,
            });

            let slots = config.waiting_capacity + usize::from(active.is_none());
            if queue.len() < slots {
                accepted += 1;
                people[idx].entry = now;
                queue.push(idx);
            } else {
                balked += 1;
                events.push(Event {
                    time: now,
                    customer: people[idx].id,
                    kind: Kind::Balk,
                });
            }
        }

        if let Some(a) = active.as_ref() {
            if now >= a.work_at {
                let best = queue.iter().copied().reduce(|best, candidate| {
                    if waiter_better(&people[candidate], &people[best]) {
                        candidate
                    } else {
                        best
                    }
                });
                if let Some(wait_idx) = best {
                    if people[wait_idx].priority < people[a.idx].priority {
                        let current = a.idx;
                        let remaining = a.finish - now;
                        let next_deadline = match people[current].patience {
                            Some(budget) => Some(now.checked_add(budget).ok_or(
                                Error::TimeOverflow {
                                    customer: people[current].id,
                                    time: now,
                                    stage: OverflowStage::Patience,
                                },
                            )?),
                            None => None,
                        };
                        let a = active.take().unwrap();
                        busy_ticks += (now - a.dispatch_time) as u128;
                        events.push(Event {
                            time: now,
                            customer: people[current].id,
                            kind: Kind::Preempt {
                                remaining,
                                by: people[wait_idx].id,
                            },
                        });
                        preemptions += 1;
                        people[current].remaining = remaining;
                        people[current].entry = now;
                        people[current].paused = true;
                        people[current].pause_time = now;
                        people[current].deadline = next_deadline;
                        queue.push(current);
                    }
                }
            }
        }

        loop {
            if active.is_some() {
                break;
            }
            let best_pos = queue
                .iter()
                .enumerate()
                .reduce(|best, candidate| {
                    if waiter_better(&people[*candidate.1], &people[*best.1]) {
                        candidate
                    } else {
                        best
                    }
                })
                .map(|(pos, _)| pos);
            let Some(pos) = best_pos else {
                break;
            };
            let idx = queue.remove(pos);
            let waited_segment = now - people[idx].entry;
            people[idx].waited += waited_segment;
            wait_ticks += waited_segment as u128;
            if let Some(budget) = people[idx].patience {
                people[idx].patience = Some(budget - waited_segment);
            }

            let switching = last_dispatch.is_some_and(|id| id != people[idx].id);
            let cost = if switching { config.switch_cost } else { 0 };
            let work_at = now.checked_add(cost).ok_or(Error::TimeOverflow {
                customer: people[idx].id,
                time: now,
                stage: OverflowStage::WorkAt,
            })?;
            let finish = work_at
                .checked_add(people[idx].remaining)
                .ok_or(Error::TimeOverflow {
                    customer: people[idx].id,
                    time: now,
                    stage: OverflowStage::Finish,
                })?;

            let is_resume = people[idx].started;
            let kind = if is_resume {
                Kind::Resume {
                    remaining: people[idx].remaining,
                    work_at,
                    finish,
                }
            } else {
                started += 1;
                Kind::Start {
                    remaining: people[idx].remaining,
                    work_at,
                    finish,
                }
            };
            people[idx].started = true;
            last_dispatch = Some(people[idx].id);
            events.push(Event {
                time: now,
                customer: people[idx].id,
                kind,
            });
            active = Some(Active {
                idx,
                work_at,
                finish,
                dispatch_time: now,
            });

            if finish == now {
                active = None;
                events.push(Event {
                    time: now,
                    customer: people[idx].id,
                    kind: Kind::Finish,
                });
                completed += 1;
                turnaround_ticks += (now - people[idx].arrival) as u128;
                completion_order.push(people[idx].id);
                continue;
            }
            break;
        }

        let mut post_expired: Vec<usize> = queue
            .iter()
            .copied()
            .filter(|&i| people[i].deadline.is_some_and(|d| d <= now))
            .collect();
        post_expired.sort_by(|&a, &b| {
            people[a]
                .deadline
                .cmp(&people[b].deadline)
                .then(people[a].id.cmp(&people[b].id))
        });
        for idx in post_expired {
            queue.retain(|&x| x != idx);
            let segment = now - people[idx].entry;
            people[idx].waited += segment;
            wait_ticks += segment as u128;
            events.push(Event {
                time: now,
                customer: people[idx].id,
                kind: Kind::Renege {
                    waited: people[idx].waited,
                },
            });
            reneged += 1;
        }

        if next_arrival >= arrival_order.len() && active.is_none() && queue.is_empty() {
            break;
        }

        let mut next_time: Option<u64> = None;
        let mut consider = |t: u64| {
            if t > now {
                next_time = Some(next_time.map_or(t, |old| old.min(t)));
            }
        };
        if next_arrival < arrival_order.len() {
            consider(people[arrival_order[next_arrival]].arrival);
        }
        if let Some(a) = active.as_ref() {
            consider(a.finish);
            consider(a.work_at);
        }
        for &idx in &queue {
            if let Some(deadline) = people[idx].deadline {
                consider(deadline);
            }
        }
        if let Some(t) = next_time {
            now = t;
        } else {
            break;
        }
    }

    let elapsed = events.last().map_or(0, |e| e.time);
    let mean_wait_milli = if accepted == 0 {
        0
    } else {
        rounded_ratio(1000u128 * wait_ticks, accepted as u128).unwrap()
    };
    let mean_turnaround_milli = if completed == 0 {
        0
    } else {
        rounded_ratio(1000u128 * turnaround_ticks, completed as u128).unwrap()
    };
    let utilization_bps = if elapsed == 0 {
        0
    } else {
        rounded_ratio(10000u128 * busy_ticks, elapsed as u128).unwrap() as u32
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

fn main() {}