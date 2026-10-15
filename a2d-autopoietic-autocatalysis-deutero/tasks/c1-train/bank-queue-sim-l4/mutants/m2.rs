use std::collections::HashSet;
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
    Renege {
        waited: u64,
    },
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
pub enum OverflowStage {
    WorkAt,
    Finish,
    Patience,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Error {
    CapacityTooLarge {
        capacity: usize,
    },
    TooManyCustomers {
        count: usize,
    },
    ZeroId {
        index: usize,
    },
    DuplicateId {
        index: usize,
        id: u32,
    },
    BadPriority {
        index: usize,
        value: u8,
    },
    DeadlineOverflow {
        index: usize,
        id: u32,
    },
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
    let q = numerator / denominator;
    let r = numerator % denominator;
    Some(q + u128::from(r > denominator - r || (r == denominator - r && q % 2 == 1)))
}
#[derive(Clone, Copy)]
struct Active {
    index: usize,
    began: u64,
    work_at: u64,
    finish: u64,
}
struct State<'a> {
    cs: &'a [Customer],
    remaining: Vec<u64>,
    budget: Vec<Option<u64>>,
    deadline: Vec<Option<u64>>,
    entered: Vec<u64>,
    waited: Vec<u64>,
    paused: Vec<bool>,
    started: Vec<bool>,
    queue: Vec<usize>,
    events: Vec<Event>,
    stats: Stats,
}
impl<'a> State<'a> {
    fn best(&self) -> Option<usize> {
        self.queue.first().copied()
    }
    fn sort_queue(&mut self) {
        self.queue.sort_by(|&a, &b| {
            self.cs[a]
                .priority
                .cmp(&self.cs[b].priority)
                .then((!self.paused[a]).cmp(&(!self.paused[b])))
                .then(self.entered[a].cmp(&self.entered[b]))
                .then_with(|| {
                    if self.paused[a] {
                        self.cs[a].id.cmp(&self.cs[b].id)
                    } else {
                        self.cs[b].id.cmp(&self.cs[a].id)
                    }
                })
        });
    }
    fn emit(&mut self, time: u64, i: usize, kind: Kind) {
        self.events.push(Event {
            time,
            customer: self.cs[i].id,
            kind,
        });
    }
    fn consume_wait(&mut self, time: u64, i: usize) {
        let duration = time - self.entered[i];
        self.waited[i] += duration;
        self.stats.wait_ticks += duration as u128;
        if let Some(b) = &mut self.budget[i] {
            *b -= duration;
        }
        self.deadline[i] = None;
    }
    fn expire(&mut self, time: u64, inclusive: bool) {
        let mut due: Vec<usize> = self
            .queue
            .iter()
            .copied()
            .filter(|&i| {
                self.deadline[i].is_some_and(|d| if inclusive { d <= time } else { d < time })
            })
            .collect();
        due.sort_by_key(|&i| (self.deadline[i].unwrap(), self.cs[i].id));
        for i in due {
            self.queue.retain(|&x| x != i);
            self.consume_wait(time, i);
            self.stats.reneged += 1;
            self.emit(
                time,
                i,
                Kind::Renege {
                    waited: self.waited[i],
                },
            );
        }
    }
    fn complete(&mut self, time: u64, i: usize) {
        self.stats.completed += 1;
        self.stats.turnaround_ticks += (time - self.cs[i].arrival) as u128;
        self.stats.completion_order.push(self.cs[i].id);
        self.emit(time, i, Kind::Finish);
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
        if c.patience
            .is_some_and(|p| c.arrival.checked_add(p).is_none())
        {
            return Err(Error::DeadlineOverflow { index, id: c.id });
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
    let mut state = State {
        cs: customers,
        remaining: customers.iter().map(|c| c.service).collect(),
        budget: customers.iter().map(|c| c.patience).collect(),
        deadline: vec![None; customers.len()],
        entered: customers.iter().map(|c| c.arrival).collect(),
        waited: vec![0; customers.len()],
        paused: vec![false; customers.len()],
        started: vec![false; customers.len()],
        queue: Vec::new(),
        events: Vec::new(),
        stats: Stats {
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
        },
    };
    let mut active: Option<Active> = None;
    let mut previous: Option<usize> = None;
    let mut next = 0;
    let mut time = 0;
    while next < order.len() || active.is_some() || !state.queue.is_empty() {
        time = order
            .get(next)
            .map(|&i| customers[i].arrival)
            .into_iter()
            .chain(active.map(|a| a.finish))
            .chain(active.filter(|a| a.work_at > time).map(|a| a.work_at))
            .chain(state.queue.iter().filter_map(|&i| state.deadline[i]))
            .min()
            .unwrap();
        // A: scheduled completion.
        if active.is_some_and(|a| a.finish == time) {
            let a = active.take().unwrap();
            state.stats.busy_ticks += (time - a.began) as u128;
            state.complete(time, a.index);
        }
        // B: customers that were waiting before this cohort.
        state.expire(time, true);
        // C: sequential admission, without dispatch between arrivals.
        while next < order.len() && customers[order[next]].arrival == time {
            let i = order[next];
            next += 1;
            let c = &customers[i];
            state.emit(time, i, Kind::Arrive);
            if state.queue.len() >= config.waiting_capacity + usize::from(active.is_none()) {
                state.stats.balked += 1;
                state.emit(time, i, Kind::Balk);
            } else {
                state.stats.accepted += 1;
                state.queue.push(i);
                state.deadline[i] = c.patience.map(|p| time + p);
            }
        }
        state.sort_queue();
        // D: preemption at service time or at the end of protected switching.
        if let (Some(a), Some(b)) = (active, state.best()) {
            if time >= a.work_at && customers[b].priority < customers[a.index].priority {
                let i = a.index;
                state.remaining[i] = a.finish - time;
                state.paused[i] = true;
                state.entered[i] = time;
                state.stats.preemptions += 1;
                state.stats.busy_ticks += (time - a.began) as u128;
                state.emit(
                    time,
                    i,
                    Kind::Preempt {
                        remaining: state.remaining[i],
                        by: customers[b].id,
                    },
                );
                state.budget[i] = customers[i].patience;
                state.deadline[i] = match state.budget[i] {
                    Some(p) => Some(time.checked_add(p).ok_or(Error::TimeOverflow {
                        customer: customers[i].id,
                        time,
                        stage: OverflowStage::Patience,
                    })?),
                    None => None,
                };
                state.queue.push(i);
                state.sort_queue();
                active = None;
            }
        }
        // E: drain inline completions; otherwise one active customer remains.
        while active.is_none() {
            let Some(i) = state.best() else {
                break;
            };
            state.queue.retain(|&x| x != i);
            state.consume_wait(time, i);
            let cost = if previous.is_some_and(|p| p != i) {
                config.switch_cost
            } else {
                0
            };
            let work_at = time.checked_add(cost).ok_or(Error::TimeOverflow {
                customer: customers[i].id,
                time,
                stage: OverflowStage::WorkAt,
            })?;
            let finish = work_at
                .checked_add(state.remaining[i])
                .ok_or(Error::TimeOverflow {
                    customer: customers[i].id,
                    time,
                    stage: OverflowStage::Finish,
                })?;
            let kind = if state.started[i] {
                Kind::Resume {
                    remaining: state.remaining[i],
                    work_at,
                    finish,
                }
            } else {
                state.stats.started += 1;
                Kind::Start {
                    remaining: state.remaining[i],
                    work_at,
                    finish,
                }
            };
            state.emit(time, i, kind);
            state.started[i] = true;
            previous = Some(i);
            if finish == time {
                state.complete(time, i);
            } else {
                active = Some(Active {
                    index: i,
                    began: time,
                    work_at,
                    finish,
                });
            }
        }
        // F: newly waiting zero-budget customers get their dispatch chance first.
        state.expire(time, true);
    }
    state.stats.elapsed = state.events.last().map_or(0, |e| e.time);
    state.stats.mean_wait_milli =
        rounded_ratio(1000 * state.stats.wait_ticks, state.stats.accepted as u128).unwrap_or(0);
    state.stats.mean_turnaround_milli = rounded_ratio(
        1000 * state.stats.turnaround_ticks,
        state.stats.completed as u128,
    )
    .unwrap_or(0);
    state.stats.utilization_bps =
        rounded_ratio(10000 * state.stats.busy_ticks, state.stats.elapsed as u128).unwrap_or(0)
            as u32;
    Ok(Report {
        events: state.events,
        stats: state.stats,
    })
}
