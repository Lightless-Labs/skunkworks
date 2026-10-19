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
    dependencies: Vec<usize>,
    state: State,
    age_from: u64,
    remaining: Option<u64>,
    segment_started: u64,
}

pub struct Pool {
    entries: Vec<Entry>,
    by_id: HashMap<String, usize>,
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

        let mut by_id = HashMap::new();
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
            if job.tag.as_ref().is_some_and(|t| !valid_token(t, 1, 8)) {
                return Err(Error::BadTag(i));
            }
            if by_id.insert(job.id.clone(), i).is_some() {
                return Err(Error::Duplicate(i));
            }
        }

        let mut resolved = Vec::with_capacity(jobs.len());
        for (i, job) in jobs.iter().enumerate() {
            let mut seen = HashSet::new();
            let mut deps = Vec::with_capacity(job.dependencies.len());
            for (j, name) in job.dependencies.iter().enumerate() {
                if !seen.insert(name.as_str()) {
                    return Err(Error::DuplicateDependency(i, j));
                }
                match by_id.get(name) {
                    Some(&index) => deps.push(index),
                    None => return Err(Error::MissingDependency(i, j)),
                }
            }
            resolved.push(deps);
        }

        let mut marks = vec![0u8; jobs.len()];
        fn visit(i: usize, deps: &[Vec<usize>], marks: &mut [u8]) -> bool {
            if marks[i] == 1 {
                return false;
            }
            if marks[i] == 2 {
                return true;
            }
            marks[i] = 1;
            for &d in &deps[i] {
                if !visit(d, deps, marks) {
                    return false;
                }
            }
            marks[i] = 2;
            true
        }
        for i in 0..jobs.len() {
            if !visit(i, &resolved, &mut marks) {
                return Err(Error::Cycle);
            }
        }

        let entries = jobs
            .into_iter()
            .enumerate()
            .map(|(i, job)| Entry {
                state: State::Waiting {
                    ready_at: job.release_at,
                    attempt: 0,
                },
                age_from: job.release_at,
                remaining: None,
                segment_started: 0,
                job,
                dependencies: resolved[i].clone(),
            })
            .collect();

        Ok(Self {
            entries,
            by_id,
            capacity,
            aging_ms,
            retry_base,
            retry_cap,
            horizon: 0,
            has_run: false,
        })
    }

    pub fn status(&self, id: &str) -> Result<State, Error> {
        self.by_id
            .get(id)
            .map(|&i| self.entries[i].state.clone())
            .ok_or(Error::Unknown)
    }

    pub fn run(&mut self, until: u64) -> Result<Vec<String>, Error> {
        if until < self.horizon {
            return Err(Error::TimeReversed);
        }
        if until > 1_000_000_000 {
            return Err(Error::TimeRange);
        }
        if self.has_run && until == self.horizon {
            return Ok(Vec::new());
        }

        let mut working = self.clone_for_run();
        let lines = working.process_run(until);
        working.horizon = until;
        working.has_run = true;
        self.entries = working.entries;
        self.horizon = working.horizon;
        self.has_run = working.has_run;
        Ok(lines)
    }

    fn clone_for_run(&self) -> Self {
        Self {
            entries: self.entries.clone(),
            by_id: self.by_id.clone(),
            capacity: self.capacity,
            aging_ms: self.aging_ms,
            retry_base: self.retry_base,
            retry_cap: self.retry_cap,
            horizon: self.horizon,
            has_run: self.has_run,
        }
    }

    fn process_run(&mut self, until: u64) -> Vec<String> {
        let mut lines = Vec::new();
        let mut now = if self.has_run { self.horizon } else { 0 };
        let initial_phase = !self.has_run;

        loop {
            let mut did_event = false;

            let mut completed: Vec<usize> = self
                .entries
                .iter()
                .enumerate()
                .filter_map(|(i, e)| match e.state {
                    State::Running { finish_at, .. } if finish_at == now => Some(i),
                    _ => None,
                })
                .collect();
            completed.sort_by(|&a, &b| self.entries[a].job.id.cmp(&self.entries[b].job.id));

            for i in completed {
                did_event = true;
                let attempt = match self.entries[i].state {
                    State::Running { attempt, .. } => attempt,
                    _ => continue,
                };
                let id = self.entries[i].job.id.clone();
                let ok = attempt > self.entries[i].job.failures;
                if ok {
                    lines.push(format!("{now} finish {id} {attempt} ok"));
                    self.entries[i].state = State::Succeeded;
                    self.entries[i].remaining = None;
                } else {
                    lines.push(format!("{now} finish {id} {attempt} fail"));
                    self.entries[i].remaining = None;
                    if attempt >= self.entries[i].job.max_attempts {
                        self.entries[i].state = State::Failed;
                    } else {
                        let delay = retry_delay(self.retry_base, self.retry_cap, attempt);
                        let ready = now.saturating_add(delay);
                        lines.push(format!("{now} retry {id} {attempt} {ready}"));
                        self.entries[i].state = State::Waiting {
                            ready_at: ready,
                            attempt,
                        };
                        self.entries[i].age_from = ready;
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
                    if self.entries[i].dependencies.iter().any(|&d| {
                        matches!(self.entries[d].state, State::Failed | State::Blocked)
                    }) {
                        self.entries[i].state = State::Blocked;
                        newly_blocked.push(i);
                        changed = true;
                    }
                }
                if !changed {
                    break;
                }
            }
            newly_blocked.sort_by(|&a, &b| self.entries[a].job.id.cmp(&self.entries[b].job.id));
            for &i in &newly_blocked {
                let dependency = self.entries[i]
                    .dependencies
                    .iter()
                    .filter(|&&d| matches!(self.entries[d].state, State::Failed | State::Blocked))
                    .map(|&d| self.entries[d].job.id.as_str())
                    .min()
                    .unwrap_or("");
                lines.push(format!(
                    "{now} skip {} {}",
                    self.entries[i].job.id, dependency
                ));
            }
            if !newly_blocked.is_empty() {
                did_event = true;
            }

            if initial_phase || did_event {
                self.schedule(now, &mut lines);
            }
            let was_initial = initial_phase;
            let _ = was_initial;

            let next = self.next_event_after(now);
            match next {
                Some(t) if t <= until => {
                    now = t;
                    // First-time scheduling at zero is consumed after this iteration.
                    // `has_run` is updated only after successful completion of the call.
                }
                _ => break,
            }
            // The initial event applies only once; later timestamps are event-driven.
            // A local marker is represented by advancing away from zero.
            if now > 0 {
                // no-op
            }
            // Avoid processing an event at the current timestamp twice.
            // Completion and ready events are fully handled by this iteration.
            // `next_event_after` is strictly greater than now.
        }
        lines
    }

    fn next_event_after(&self, now: u64) -> Option<u64> {
        let mut next: Option<u64> = None;
        for e in &self.entries {
            let t = match e.state {
                State::Waiting { ready_at, .. } if ready_at > now => Some(ready_at),
                State::Running { finish_at, .. } if finish_at > now => Some(finish_at),
                _ => None,
            };
            if let Some(t) = t {
                next = Some(next.map_or(t, |n| n.min(t)));
            }
        }
        next
    }

    fn schedule(&mut self, now: u64, lines: &mut Vec<String>) {
        loop {
            let mut candidates: Vec<usize> = (0..self.entries.len())
                .filter(|&i| self.is_eligible(i, now))
                .collect();
            candidates.sort_by(|&a, &b| {
                let pa = self.effective_priority(a, now);
                let pb = self.effective_priority(b, now);
                pb.cmp(&pa).then_with(|| a.cmp(&b))
            });

            let mut selected = None;
            for candidate in candidates {
                let running: Vec<usize> = self
                    .entries
                    .iter()
                    .enumerate()
                    .filter_map(|(i, e)| matches!(e.state, State::Running { .. }).then_some(i))
                    .collect();
                let candidate_tag = self.entries[candidate].job.tag.as_deref();
                let slots_free = running.len() < self.capacity;
                let tag_free = candidate_tag.is_none()
                    || !running.iter().any(|&i| {
                        self.entries[i].job.tag.as_deref() == candidate_tag
                    });

                if slots_free && tag_free {
                    selected = Some((candidate, None));
                    break;
                }

                let mut victims = Vec::new();
                for &victim in &running {
                    if self.entries[candidate].job.priority
                        < self.entries[victim].job.priority.saturating_add(2)
                    {
                        continue;
                    }
                    let remaining_tag_busy = running.iter().any(|&other| {
                        other != victim
                            && candidate_tag.is_some()
                            && self.entries[other].job.tag.as_deref() == candidate_tag
                    });
                    if remaining_tag_busy {
                        continue;
                    }
                    if running.len() - 1 >= self.capacity {
                        continue;
                    }
                    victims.push(victim);
                }
                victims.sort_by(|&a, &b| {
                    self.entries[a]
                        .job
                        .priority
                        .cmp(&self.entries[b].job.priority)
                        .then_with(|| {
                            self.entries[b]
                                .segment_started
                                .cmp(&self.entries[a].segment_started)
                        })
                        .then_with(|| self.entries[b].job.id.cmp(&self.entries[a].job.id))
                });
                if let Some(victim) = victims.first().copied() {
                    selected = Some((candidate, Some(victim)));
                    break;
                }
            }

            let Some((candidate, victim)) = selected else {
                break;
            };

            if let Some(victim) = victim {
                let finish_at = match self.entries[victim].state {
                    State::Running { finish_at, .. } => finish_at,
                    _ => now,
                };
                let attempt = match self.entries[victim].state {
                    State::Running { attempt, .. } => attempt,
                    _ => 0,
                };
                let remaining = finish_at.saturating_sub(now);
                let victim_id = self.entries[victim].job.id.clone();
                let candidate_id = self.entries[candidate].job.id.clone();
                lines.push(format!("{now} preempt {victim_id} by {candidate_id}"));
                self.entries[victim].state = State::Waiting {
                    ready_at: now,
                    attempt,
                };
                self.entries[victim].age_from = now;
                self.entries[victim].remaining = Some(remaining);
            }

            let id = self.entries[candidate].job.id.clone();
            let (attempt, remaining) = if let Some(remaining) = self.entries[candidate].remaining {
                let attempt = match self.entries[candidate].state {
                    State::Waiting { attempt, .. } => attempt,
                    _ => 0,
                };
                self.entries[candidate].remaining = None;
                (attempt, remaining)
            } else {
                let attempt = match self.entries[candidate].state {
                    State::Waiting { attempt, .. } => attempt.saturating_add(1),
                    _ => 1,
                };
                (attempt, self.entries[candidate].job.duration)
            };
            let finish_at = now.saturating_add(remaining);
            self.entries[candidate].state = State::Running { attempt, finish_at };
            self.entries[candidate].segment_started = now;
            lines.push(format!("{now} start {id} {attempt} {remaining}"));
        }
    }

    fn is_eligible(&self, i: usize, now: u64) -> bool {
        match self.entries[i].state {
            State::Waiting { ready_at, .. } if ready_at <= now => self.entries[i]
                .dependencies
                .iter()
                .all(|&d| matches!(self.entries[d].state, State::Succeeded)),
            _ => false,
        }
    }

    fn effective_priority(&self, i: usize, now: u64) -> u8 {
        if self.aging_ms == 0 {
            return self.entries[i].job.priority;
        }
        let elapsed = now.saturating_sub(self.entries[i].age_from);
        let gain = elapsed / self.aging_ms;
        (self.entries[i].job.priority as u64)
            .saturating_add(gain)
            .min(9) as u8
    }
}

fn valid_token(s: &str, min_len: usize, max_len: usize) -> bool {
    let bytes = s.as_bytes();
    (min_len..=max_len).contains(&bytes.len())
        && bytes.iter().all(|b| {
            b.is_ascii_lowercase() || b.is_ascii_digit() || *b == b'-'
        })
}

fn retry_delay(base: u64, cap: u64, attempt: u8) -> u64 {
    let mut delay = base;
    for _ in 1..attempt {
        if delay >= cap {
            return cap;
        }
        delay = delay.saturating_mul(2).min(cap);
    }
    delay.min(cap)
}