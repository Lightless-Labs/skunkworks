use std::collections::{HashMap, HashSet};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Job {
    pub id: String,
    pub priority: u8,
    pub dependencies: Vec<String>,
    pub max_attempts: u8,
    pub release_at: u64,
    pub duration: u64,
    pub tag: Option<String>,
    pub failures: u8,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum State {
    Waiting { ready_at: u64, attempt: u8 },
    Running { attempt: u8, finish_at: u64 },
    Succeeded,
    Failed,
    Blocked,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Error {
    Config,
    BadId(usize),
    BadPriority(usize),
    BadAttempts(usize),
    BadTime(usize),
    BadTag(usize),
    Duplicate(usize),
    DuplicateDependency(usize, usize),
    MissingDependency(usize, usize),
    Cycle,
    TimeReversed,
    TimeRange,
    Unknown,
}

#[derive(Clone)]
struct Entry {
    job: Job,
    state: State,
    remaining: Option<u64>,
    started_at: u64,
    input_order: usize,
}

pub struct Pool {
    entries: Vec<Entry>,
    index: HashMap<String, usize>,
    capacity: usize,
    aging_ms: u64,
    retry_base: u64,
    retry_cap: u64,
    horizon: u64,
    has_run: bool,
}

impl Pool {
    pub fn new(
        jobs: Vec<Job>,
        capacity: usize,
        aging_ms: u64,
        retry_base: u64,
        retry_cap: u64,
    ) -> Result<Self, Error> {
        if !(1..=16).contains(&capacity)
            || retry_base == 0
            || retry_base > retry_cap
            || retry_cap > 1_000_000_000
        {
            return Err(Error::Config);
        }

        let mut index = HashMap::new();

        for (i, job) in jobs.iter().enumerate() {
            if job.id.is_empty()
                || job.id.len() > 16
                || !job.id.bytes().all(valid_id_byte)
            {
                return Err(Error::BadId(i));
            }
            if job.priority > 9 {
                return Err(Error::BadPriority(i));
            }
            if !(1..=32).contains(&job.max_attempts) {
                return Err(Error::BadAttempts(i));
            }
            if job.release_at > 1_000_000_000
                || job.duration == 0
                || job.duration > 1_000_000_000
            {
                return Err(Error::BadTime(i));
            }
            if let Some(tag) = &job.tag {
                if tag.is_empty() || tag.len() > 8 || !tag.bytes().all(valid_id_byte) {
                    return Err(Error::BadTag(i));
                }
            }
            if index.contains_key(&job.id) {
                return Err(Error::Duplicate(i));
            }
            index.insert(job.id.clone(), i);
        }

        let mut dependencies = vec![Vec::new(); jobs.len()];
        for (i, job) in jobs.iter().enumerate() {
            let mut seen = HashSet::new();
            for (j, dep) in job.dependencies.iter().enumerate() {
                if !seen.insert(dep.as_str()) {
                    return Err(Error::DuplicateDependency(i, j));
                }
                let Some(&dep_index) = index.get(dep) else {
                    return Err(Error::MissingDependency(i, j));
                };
                dependencies[i].push(dep_index);
            }
        }

        fn visit(
            node: usize,
            deps: &[Vec<usize>],
            marks: &mut [u8],
        ) -> bool {
            if marks[node] == 1 {
                return false;
            }
            if marks[node] == 2 {
                return true;
            }
            marks[node] = 1;
            for &dep in &deps[node] {
                if !visit(dep, deps, marks) {
                    return false;
                }
            }
            marks[node] = 2;
            true
        }

        let mut marks = vec![0; jobs.len()];
        for i in 0..jobs.len() {
            if !visit(i, &dependencies, &mut marks) {
                return Err(Error::Cycle);
            }
        }

        let entries = jobs
            .into_iter()
            .enumerate()
            .map(|(input_order, job)| Entry {
                state: State::Waiting {
                    ready_at: job.release_at,
                    attempt: 0,
                },
                job,
                remaining: None,
                started_at: 0,
                input_order,
            })
            .collect();

        Ok(Self {
            entries,
            index,
            capacity,
            aging_ms,
            retry_base,
            retry_cap,
            horizon: 0,
            has_run: false,
        })
    }

    pub fn run(&mut self, until: u64) -> Result<Vec<String>, Error> {
        if self.has_run && until < self.horizon {
            return Err(Error::TimeReversed);
        }
        if until > 1_000_000_000 {
            return Err(Error::TimeRange);
        }
        if self.has_run && until == self.horizon {
            return Ok(Vec::new());
        }

        let mut events = Vec::new();
        let mut time = if self.has_run {
            self.next_event_after(self.horizon, until)
        } else {
            Some(0)
        };

        while let Some(t) = time {
            self.process_time(t, &mut events);
            time = self.next_event_after(t, until);
        }

        self.horizon = until;
        self.has_run = true;
        Ok(events)
    }

    pub fn status(&self, id: &str) -> Result<State, Error> {
        let Some(&i) = self.index.get(id) else {
            return Err(Error::Unknown);
        };
        Ok(self.entries[i].state.clone())
    }

    fn next_event_after(&self, after: u64, until: u64) -> Option<u64> {
        let mut next = None;
        for entry in &self.entries {
            let candidate = match entry.state {
                State::Waiting { ready_at, .. } => Some(ready_at),
                State::Running { finish_at, .. } => Some(finish_at),
                _ => None,
            };
            if let Some(t) = candidate {
                if t > after && t <= until {
                    next = Some(next.map_or(t, |current: u64| current.min(t)));
                }
            }
        }
        next
    }

    fn process_time(&mut self, t: u64, events: &mut Vec<String>) {
        let mut finished: Vec<usize> = self
            .entries
            .iter()
            .enumerate()
            .filter_map(|(i, e)| match e.state {
                State::Running { finish_at, .. } if finish_at == t => Some(i),
                _ => None,
            })
            .collect();

        finished.sort_by(|&a, &b| self.entries[a].job.id.as_bytes().cmp(self.entries[b].job.id.as_bytes()));

        for i in finished {
            let attempt = match self.entries[i].state {
                State::Running { attempt, .. } => attempt,
                _ => continue,
            };
            let ok = attempt > self.entries[i].job.failures;
            if ok {
                self.entries[i].state = State::Succeeded;
                events.push(format!("{} finish {} {} ok", t, self.entries[i].job.id, attempt));
            } else {
                events.push(format!("{} finish {} {} fail", t, self.entries[i].job.id, attempt));
                if attempt >= self.entries[i].job.max_attempts {
                    self.entries[i].state = State::Failed;
                } else {
                    let shift = u32::from(attempt.saturating_sub(1));
                    let delay = self.retry_base.saturating_mul(1u64.checked_shl(shift).unwrap_or(u64::MAX))
                        .min(self.retry_cap);
                    let ready_at = t.saturating_add(delay);
                    self.entries[i].state = State::Waiting { ready_at, attempt };
                    events.push(format!(
                        "{} retry {} {} {}",
                        t, self.entries[i].job.id, attempt, ready_at
                    ));
                }
            }
        }

        let mut newly_blocked = Vec::new();
        loop {
            let mut changed = false;
            for i in 0..self.entries.len() {
                if !matches!(self.entries[i].state, State::Waiting { .. }) {
                    continue;
                }
                let blocked_by = self.entries[i]
                    .job
                    .dependencies
                    .iter()
                    .find(|dep| {
                        let &di = &self.index[*dep];
                        matches!(self.entries[di].state, State::Failed | State::Blocked)
                    })
                    .cloned();
                if let Some(dep) = blocked_by {
                    self.entries[i].state = State::Blocked;
                    newly_blocked.push((i, dep));
                    changed = true;
                }
            }
            if !changed {
                break;
            }
        }
        newly_blocked.sort_by(|(a, _), (b, _)| {
            self.entries[*a].job.id.as_bytes().cmp(self.entries[*b].job.id.as_bytes())
        });
        for (i, dep) in newly_blocked {
            events.push(format!("{} skip {} {}", t, self.entries[i].job.id, dep));
        }

        self.schedule(t, events);
    }

    fn schedule(&mut self, t: u64, events: &mut Vec<String>) {
        loop {
            let mut candidates: Vec<usize> = self
                .entries
                .iter()
                .enumerate()
                .filter_map(|(i, e)| {
                    let State::Waiting { ready_at, .. } = e.state else {
                        return None;
                    };
                    if ready_at > t {
                        return None;
                    }
                    let ready = e.job.dependencies.iter().all(|dep| {
                        matches!(self.entries[self.index[dep]].state, State::Succeeded)
                    });
                    ready.then_some(i)
                })
                .collect();

            candidates.sort_by(|&a, &b| {
                let pa = self.effective_priority(a, t);
                let pb = self.effective_priority(b, t);
                pb.cmp(&pa).then_with(|| {
                    self.entries[a].input_order.cmp(&self.entries[b].input_order)
                })
            });

            let mut action = None;
            for candidate in candidates {
                let tag = self.entries[candidate].job.tag.as_deref();
                let running: Vec<usize> = self
                    .entries
                    .iter()
                    .enumerate()
                    .filter_map(|(i, e)| matches!(e.state, State::Running { .. }).then_some(i))
                    .collect();

                let slot_free = running.len() < self.capacity;
                let tag_free = tag.is_none()
                    || !running.iter().any(|&i| self.entries[i].job.tag.as_deref() == tag);

                if slot_free && tag_free {
                    action = Some((candidate, None));
                    break;
                }

                let mut victims: Vec<usize> = running
                    .iter()
                    .copied()
                    .filter(|&victim| {
                        self.entries[candidate].job.priority
                            >= self.entries[victim].job.priority.saturating_add(2)
                            && (tag.is_none()
                                || self.entries[victim].job.tag.as_deref() == tag
                                || !running.iter().any(|&other| {
                                    other != victim
                                        && self.entries[other].job.tag.as_deref() == tag
                                }))
                    })
                    .collect();

                victims.sort_by(|&a, &b| {
                    self.entries[a]
                        .job
                        .priority
                        .cmp(&self.entries[b].job.priority)
                        .then_with(|| self.entries[b].started_at.cmp(&self.entries[a].started_at))
                        .then_with(|| {
                            self.entries[b]
                                .job
                                .id
                                .as_bytes()
                                .cmp(self.entries[a].job.id.as_bytes())
                        })
                });

                if let Some(victim) = victims.first().copied() {
                    action = Some((candidate, Some(victim)));
                    break;
                }
            }

            let Some((candidate, victim)) = action else {
                break;
            };

            if let Some(victim) = victim {
                let (attempt, finish_at) = match self.entries[victim].state {
                    State::Running { attempt, finish_at } => (attempt, finish_at),
                    _ => continue,
                };
                let remaining = finish_at.saturating_sub(t);
                self.entries[victim].remaining = Some(remaining);
                self.entries[victim].state = State::Waiting { ready_at: t, attempt };
                events.push(format!(
                    "{} preempt {} by {}",
                    t, self.entries[victim].job.id, self.entries[candidate].job.id
                ));
            }

            let (attempt, remaining) = match self.entries[candidate].state {
                State::Waiting { attempt, .. } => {
                    if let Some(saved) = self.entries[candidate].remaining.take() {
                        (attempt, saved)
                    } else {
                        let next_attempt = attempt.saturating_add(1);
                        (next_attempt, self.entries[candidate].job.duration)
                    }
                }
                _ => continue,
            };

            let finish_at = t.saturating_add(remaining);
            self.entries[candidate].state = State::Running { attempt, finish_at };
            self.entries[candidate].started_at = t;
            events.push(format!(
                "{} start {} {} {}",
                t, self.entries[candidate].job.id, attempt, remaining
            ));
        }
    }

    fn effective_priority(&self, i: usize, t: u64) -> u8 {
        if self.aging_ms == 0 {
            return self.entries[i].job.priority;
        }
        let ready_at = match self.entries[i].state {
            State::Waiting { ready_at, .. } => ready_at,
            _ => t,
        };
        let age = t.saturating_sub(ready_at) / self.aging_ms;
        (u64::from(self.entries[i].job.priority) + age).min(9) as u8
    }
}

fn valid_id_byte(b: u8) -> bool {
    b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-'
}