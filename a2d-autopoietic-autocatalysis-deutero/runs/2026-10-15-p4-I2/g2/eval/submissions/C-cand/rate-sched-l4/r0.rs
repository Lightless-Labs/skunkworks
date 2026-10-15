use std::collections::HashMap;

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
struct Runtime {
    state: State,
    remaining: Option<u64>,
    segment_start: u64,
}

pub struct Pool {
    jobs: Vec<Job>,
    states: Vec<Runtime>,
    ids: HashMap<String, usize>,
    dependencies: Vec<Vec<usize>>,
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

        let mut ids = HashMap::new();
        for (i, job) in jobs.iter().enumerate() {
            if !valid_token(&job.id, 1, 16) {
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
            if job.tag.as_ref().is_some_and(|s| !valid_token(s, 1, 8)) {
                return Err(Error::BadTag(i));
            }
            if ids.insert(job.id.clone(), i).is_some() {
                return Err(Error::Duplicate(i));
            }
        }

        let mut dependencies = vec![Vec::new(); jobs.len()];
        for (i, job) in jobs.iter().enumerate() {
            let mut seen = HashMap::<&str, ()>::new();
            for (j, spelling) in job.dependencies.iter().enumerate() {
                if seen.insert(spelling.as_str(), ()).is_some() {
                    return Err(Error::DuplicateDependency(i, j));
                }
                match ids.get(spelling) {
                    Some(&index) => dependencies[i].push(index),
                    None => return Err(Error::MissingDependency(i, j)),
                }
            }
        }

        let mut color = vec![0u8; jobs.len()];
        fn visit(v: usize, deps: &[Vec<usize>], color: &mut [u8]) -> bool {
            if color[v] == 1 {
                return true;
            }
            if color[v] == 2 {
                return false;
            }
            color[v] = 1;
            for &w in &deps[v] {
                if visit(w, deps, color) {
                    return true;
                }
            }
            color[v] = 2;
            false
        }
        for i in 0..jobs.len() {
            if visit(i, &dependencies, &mut color) {
                return Err(Error::Cycle);
            }
        }

        let states = jobs
            .iter()
            .map(|job| Runtime {
                state: State::Waiting {
                    ready_at: job.release_at,
                    attempt: 0,
                },
                remaining: None,
                segment_start: 0,
            })
            .collect();

        Ok(Self {
            jobs,
            states,
            ids,
            dependencies,
            capacity,
            aging_ms,
            retry_base,
            retry_cap,
            horizon: 0,
            has_run: false,
        })
    }

    pub fn run(&mut self, until: u64) -> Result<Vec<String>, Error> {
        if until < self.horizon {
            return Err(Error::TimeReversed);
        }
        if until > 1_000_000_000 {
            return Err(Error::TimeRange);
        }

        let mut lines = Vec::new();
        let mut t = if self.has_run {
            self.horizon
        } else {
            0
        };

        if !self.has_run {
            self.process_timestamp(t, &mut lines);
        }

        loop {
            let Some(next) = self.next_event_time(t) else {
                break;
            };
            if next > until {
                break;
            }
            t = next;
            self.process_timestamp(t, &mut lines);
        }

        self.horizon = until;
        self.has_run = true;
        Ok(lines)
    }

    pub fn status(&self, id: &str) -> Result<State, Error> {
        let Some(&index) = self.ids.get(id) else {
            return Err(Error::Unknown);
        };
        Ok(self.states[index].state.clone())
    }

    fn next_event_time(&self, now: u64) -> Option<u64> {
        let mut next: Option<u64> = None;
        for runtime in &self.states {
            let time = match runtime.state {
                State::Waiting { ready_at, .. } if ready_at > now => Some(ready_at),
                State::Running { finish_at, .. } if finish_at > now => Some(finish_at),
                _ => None,
            };
            if let Some(time) = time {
                next = Some(next.map_or(time, |n| n.min(time)));
            }
        }
        next
    }

    fn process_timestamp(&mut self, t: u64, lines: &mut Vec<String>) {
        let mut completions: Vec<usize> = self
            .states
            .iter()
            .enumerate()
            .filter_map(|(i, r)| match r.state {
                State::Running { finish_at, .. } if finish_at == t => Some(i),
                _ => None,
            })
            .collect();
        completions.sort_by(|&a, &b| self.jobs[a].id.as_bytes().cmp(self.jobs[b].id.as_bytes()));

        for i in completions {
            let attempt = match self.states[i].state {
                State::Running { attempt, .. } => attempt,
                _ => continue,
            };
            let succeeded = attempt > self.jobs[i].failures;
            if succeeded {
                lines.push(format!("{} finish {} {} ok", t, self.jobs[i].id, attempt));
                self.states[i].state = State::Succeeded;
                self.states[i].remaining = None;
            } else {
                lines.push(format!("{} finish {} {} fail", t, self.jobs[i].id, attempt));
                self.states[i].remaining = None;
                if attempt >= self.jobs[i].max_attempts {
                    self.states[i].state = State::Failed;
                } else {
                    let delay = retry_delay(self.retry_base, self.retry_cap, attempt);
                    let ready_at = t.saturating_add(delay);
                    lines.push(format!(
                        "{} retry {} {} {}",
                        t, self.jobs[i].id, attempt, ready_at
                    ));
                    self.states[i].state = State::Waiting { ready_at, attempt };
                }
            }
        }

        let mut newly_blocked = Vec::new();
        loop {
            let mut changed = false;
            for i in 0..self.jobs.len() {
                if !matches!(self.states[i].state, State::Waiting { .. }) {
                    continue;
                }
                if self.dependencies[i].iter().any(|&d| {
                    matches!(self.states[d].state, State::Failed | State::Blocked)
                }) {
                    self.states[i].state = State::Blocked;
                    self.states[i].remaining = None;
                    newly_blocked.push(i);
                    changed = true;
                }
            }
            if !changed {
                break;
            }
        }
        newly_blocked.sort_by(|&a, &b| self.jobs[a].id.as_bytes().cmp(self.jobs[b].id.as_bytes()));
        for i in newly_blocked {
            let failed_dep = self.dependencies[i]
                .iter()
                .filter(|&&d| matches!(self.states[d].state, State::Failed | State::Blocked))
                .min_by(|&&a, &&b| self.jobs[a].id.as_bytes().cmp(self.jobs[b].id.as_bytes()))
                .copied();
            if let Some(d) = failed_dep {
                lines.push(format!("{} skip {} {}", t, self.jobs[i].id, self.jobs[d].id));
            }
        }

        loop {
            let running: Vec<usize> = self
                .states
                .iter()
                .enumerate()
                .filter_map(|(i, r)| matches!(r.state, State::Running { .. }).then_some(i))
                .collect();

            let mut candidates: Vec<usize> = (0..self.jobs.len())
                .filter(|&i| {
                    matches!(self.states[i].state, State::Waiting { ready_at, .. } if ready_at <= t)
                        && self.dependencies[i]
                            .iter()
                            .all(|&d| matches!(self.states[d].state, State::Succeeded))
                })
                .collect();

            candidates.sort_by(|&a, &b| {
                let pa = self.effective_priority(a, t);
                let pb = self.effective_priority(b, t);
                pb.cmp(&pa).then_with(|| a.cmp(&b))
            });

            let mut chosen: Option<(usize, Option<usize>)> = None;
            for candidate in candidates {
                let candidate_tag = self.jobs[candidate].tag.as_deref();
                let tag_busy = candidate_tag.is_some_and(|tag| {
                    running.iter().any(|&r| self.jobs[r].tag.as_deref() == Some(tag))
                });
                if running.len() < self.capacity && !tag_busy {
                    chosen = Some((candidate, None));
                    break;
                }

                let mut victims: Vec<usize> = running
                    .iter()
                    .copied()
                    .filter(|&v| {
                        self.jobs[candidate].priority >= self.jobs[v].priority.saturating_add(2)
                            && {
                                let mut remaining: Vec<usize> = running
                                    .iter()
                                    .copied()
                                    .filter(|&r| r != v)
                                    .collect();
                                let slot_free = remaining.len() < self.capacity;
                                let tag_free = candidate_tag.is_none()
                                    || !remaining.iter().any(|&r| {
                                        self.jobs[r].tag.as_deref() == candidate_tag
                                    });
                                slot_free && tag_free
                            }
                    })
                    .collect();

                victims.sort_by(|&a, &b| {
                    self.jobs[a]
                        .priority
                        .cmp(&self.jobs[b].priority)
                        .then_with(|| {
                            let sa = self.states[a].segment_start;
                            let sb = self.states[b].segment_start;
                            sb.cmp(&sa)
                        })
                        .then_with(|| self.jobs[b].id.as_bytes().cmp(self.jobs[a].id.as_bytes()))
                });

                if let Some(victim) = victims.first().copied() {
                    chosen = Some((candidate, Some(victim)));
                    break;
                }
            }

            let Some((candidate, victim)) = chosen else {
                break;
            };

            if let Some(victim) = victim {
                let (attempt, finish_at) = match self.states[victim].state {
                    State::Running { attempt, finish_at } => (attempt, finish_at),
                    _ => continue,
                };
                let remaining = finish_at - t;
                lines.push(format!(
                    "{} preempt {} by {}",
                    t, self.jobs[victim].id, self.jobs[candidate].id
                ));
                self.states[victim].state = State::Waiting {
                    ready_at: t,
                    attempt,
                };
                self.states[victim].remaining = Some(remaining);
            }

            let (ready_at, attempt) = match self.states[candidate].state {
                State::Waiting { ready_at, attempt } => (ready_at, attempt),
                _ => continue,
            };
            let resumed = self.states[candidate].remaining.take();
            let (new_attempt, duration) = if let Some(remaining) = resumed {
                (attempt, remaining)
            } else {
                let next_attempt = attempt.saturating_add(1);
                (next_attempt, self.jobs[candidate].duration)
            };
            let finish_at = t.saturating_add(duration);
            self.states[candidate].state = State::Running {
                attempt: new_attempt,
                finish_at,
            };
            self.states[candidate].segment_start = t;
            let _ = ready_at;
            lines.push(format!(
                "{} start {} {} {}",
                t, self.jobs[candidate].id, new_attempt, duration
            ));
        }
    }

    fn effective_priority(&self, index: usize, now: u64) -> u8 {
        let base = self.jobs[index].priority;
        if self.aging_ms == 0 {
            return base;
        }
        let since = match self.states[index].state {
            State::Waiting { ready_at, .. } => ready_at,
            _ => now,
        };
        let increment = (now - since) / self.aging_ms;
        (base as u64 + increment).min(9) as u8
    }
}

fn valid_token(s: &str, min: usize, max: usize) -> bool {
    (min..=max).contains(&s.len())
        && s.bytes().all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
}

fn retry_delay(base: u64, cap: u64, attempt: u8) -> u64 {
    let mut delay = base;
    for _ in 1..attempt {
        if delay >= cap || delay > cap / 2 {
            return cap;
        }
        delay *= 2;
    }
    delay.min(cap)
}

fn main() {}