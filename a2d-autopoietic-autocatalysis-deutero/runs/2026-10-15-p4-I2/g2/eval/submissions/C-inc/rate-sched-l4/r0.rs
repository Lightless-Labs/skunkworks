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

struct Entry {
    job: Job,
    state: State,
    deps: Vec<usize>,
    resume_remaining: Option<u64>,
    started_at: u64,
}

pub struct Pool {
    entries: Vec<Entry>,
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
            || retry_base < 1
            || retry_base > retry_cap
            || retry_cap > 1_000_000_000
        {
            return Err(Error::Config);
        }

        let mut ids = std::collections::HashMap::<String, usize>::new();

        for (i, job) in jobs.iter().enumerate() {
            let valid_id = !job.id.is_empty()
                && job.id.len() <= 16
                && job
                    .id
                    .bytes()
                    .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-');
            if !valid_id {
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
                let valid_tag = !tag.is_empty()
                    && tag.len() <= 8
                    && tag
                        .bytes()
                        .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-');
                if !valid_tag {
                    return Err(Error::BadTag(i));
                }
            }
            if ids.contains_key(&job.id) {
                return Err(Error::Duplicate(i));
            }
            ids.insert(job.id.clone(), i);
        }

        let mut deps = vec![Vec::new(); jobs.len()];
        for (i, job) in jobs.iter().enumerate() {
            let mut seen = std::collections::HashSet::<String>::new();
            for (j, dep) in job.dependencies.iter().enumerate() {
                if !seen.insert(dep.clone()) {
                    return Err(Error::DuplicateDependency(i, j));
                }
                match ids.get(dep) {
                    Some(&index) => deps[i].push(index),
                    None => return Err(Error::MissingDependency(i, j)),
                }
            }
        }

        fn visit(i: usize, deps: &[Vec<usize>], marks: &mut [u8]) -> bool {
            match marks[i] {
                1 => return false,
                2 => return true,
                _ => {}
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

        let mut marks = vec![0u8; jobs.len()];
        for i in 0..jobs.len() {
            if !visit(i, &deps, &mut marks) {
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
                job,
                deps: deps[i].clone(),
                resume_remaining: None,
                started_at: 0,
            })
            .collect();

        Ok(Self {
            entries,
            capacity,
            aging_ms,
            retry_base,
            retry_cap,
            horizon: 0,
            has_run: false,
        })
    }

    pub fn status(&self, id: &str) -> Result<State, Error> {
        self.entries
            .iter()
            .find(|e| e.job.id == id)
            .map(|e| e.state.clone())
            .ok_or(Error::Unknown)
    }

    fn dependencies_succeeded(&self, i: usize) -> bool {
        self.entries[i]
            .deps
            .iter()
            .all(|&d| matches!(self.entries[d].state, State::Succeeded))
    }

    fn occupied_tag(&self, tag: &str) -> bool {
        self.entries.iter().any(|e| {
            matches!(e.state, State::Running { .. }) && e.job.tag.as_deref() == Some(tag)
        })
    }

    fn running_count(&self) -> usize {
        self.entries
            .iter()
            .filter(|e| matches!(e.state, State::Running { .. }))
            .count()
    }

    fn effective_priority(&self, i: usize, now: u64) -> u8 {
        let base = self.entries[i].job.priority;
        if self.aging_ms == 0 {
            return base;
        }
        let ready_at = match self.entries[i].state {
            State::Waiting { ready_at, .. } => ready_at,
            _ => now,
        };
        let age = now.saturating_sub(ready_at) / self.aging_ms;
        (base as u64 + age).min(9) as u8
    }

    fn next_event_after(&self, now: u64) -> Option<u64> {
        self.entries
            .iter()
            .filter_map(|e| match e.state {
                State::Running { finish_at, .. } if finish_at > now => Some(finish_at),
                State::Waiting { ready_at, .. } if ready_at > now => Some(ready_at),
                _ => None,
            })
            .min()
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

        let mut trace = Vec::new();
        let mut now = if self.has_run { self.horizon } else { 0 };

        if self.has_run {
            match self.next_event_after(now) {
                Some(next) if next <= until => now = next,
                _ => {
                    self.horizon = until;
                    return Ok(trace);
                }
            }
        }

        loop {
            if now > until {
                break;
            }

            let mut completions: Vec<usize> = self
                .entries
                .iter()
                .enumerate()
                .filter_map(|(i, e)| match e.state {
                    State::Running { finish_at, .. } if finish_at == now => Some(i),
                    _ => None,
                })
                .collect();
            completions.sort_by(|&a, &b| self.entries[a].job.id.cmp(&self.entries[b].job.id));

            for i in completions {
                let attempt = match self.entries[i].state {
                    State::Running { attempt, .. } => attempt,
                    _ => continue,
                };
                if attempt > self.entries[i].job.failures {
                    self.entries[i].state = State::Succeeded;
                    trace.push(format!(
                        "{} finish {} {} ok",
                        now, self.entries[i].job.id, attempt
                    ));
                } else {
                    trace.push(format!(
                        "{} finish {} {} fail",
                        now, self.entries[i].job.id, attempt
                    ));
                    if attempt >= self.entries[i].job.max_attempts {
                        self.entries[i].state = State::Failed;
                    } else {
                        let shift = (attempt - 1) as u32;
                        let delay = self
                            .retry_base
                            .checked_mul(1u64.checked_shl(shift).unwrap_or(u64::MAX))
                            .unwrap_or(u64::MAX)
                            .min(self.retry_cap);
                        let ready = now.saturating_add(delay);
                        self.entries[i].state = State::Waiting {
                            ready_at: ready,
                            attempt,
                        };
                        self.entries[i].resume_remaining = None;
                        trace.push(format!(
                            "{} retry {} {} {}",
                            now, self.entries[i].job.id, attempt, ready
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
                    let blocked = self.entries[i].deps.iter().any(|&d| {
                        matches!(self.entries[d].state, State::Failed | State::Blocked)
                    });
                    if blocked {
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
            for i in newly_blocked {
                let failed_dep = self.entries[i]
                    .deps
                    .iter()
                    .find(|&&d| {
                        matches!(self.entries[d].state, State::Failed | State::Blocked)
                    })
                    .copied();
                if let Some(d) = failed_dep {
                    trace.push(format!(
                        "{} skip {} {}",
                        now, self.entries[i].job.id, self.entries[d].job.id
                    ));
                }
            }

            loop {
                let mut candidates: Vec<usize> = (0..self.entries.len())
                    .filter(|&i| {
                        matches!(self.entries[i].state, State::Waiting { ready_at, .. } if ready_at <= now)
                            && self.dependencies_succeeded(i)
                    })
                    .collect();
                candidates.sort_by(|&a, &b| {
                    self.effective_priority(b, now)
                        .cmp(&self.effective_priority(a, now))
                        .then_with(|| a.cmp(&b))
                });

                let mut acted = false;
                for candidate in candidates {
                    let tag = self.entries[candidate].job.tag.as_deref();
                    let slot_free = self.running_count() < self.capacity;
                    let tag_free = tag.map(|t| !self.occupied_tag(t)).unwrap_or(true);

                    let victim = if slot_free && tag_free {
                        None
                    } else {
                        let running_count = self.running_count();
                        let mut victims: Vec<usize> = self
                            .entries
                            .iter()
                            .enumerate()
                            .filter_map(|(i, e)| {
                                if !matches!(e.state, State::Running { .. }) {
                                    return None;
                                }
                                if self.entries[candidate].job.priority
                                    < e.job.priority.saturating_add(2)
                                {
                                    return None;
                                }

                                let tag_becomes_free = tag
                                    .map(|t| {
                                        e.job.tag.as_deref() == Some(t)
                                            && !self.entries.iter().enumerate().any(|(j, other)| {
                                                j != i
                                                    && matches!(other.state, State::Running { .. })
                                                    && other.job.tag.as_deref() == Some(t)
                                            })
                                    })
                                    .unwrap_or(true);
                                let slot_becomes_free =
                                    running_count.saturating_sub(1) < self.capacity;
                                if !slot_becomes_free || !tag_becomes_free {
                                    return None;
                                }
                                Some(i)
                            })
                            .collect();

                        victims.sort_by(|&a, &b| {
                            self.entries[a]
                                .job
                                .priority
                                .cmp(&self.entries[b].job.priority)
                                .then_with(|| {
                                    self.entries[b].started_at.cmp(&self.entries[a].started_at)
                                })
                                .then_with(|| self.entries[b].job.id.cmp(&self.entries[a].job.id))
                        });
                        victims.first().copied()
                    };

                    if !slot_free || !tag_free {
                        let Some(victim) = victim else {
                            continue;
                        };
                        let (attempt, finish_at) = match self.entries[victim].state {
                            State::Running {
                                attempt,
                                finish_at,
                            } => (attempt, finish_at),
                            _ => continue,
                        };
                        let remaining = finish_at.saturating_sub(now);
                        let victim_id = self.entries[victim].job.id.clone();
                        let candidate_id = self.entries[candidate].job.id.clone();
                        trace.push(format!(
                            "{} preempt {} by {}",
                            now, victim_id, candidate_id
                        ));
                        self.entries[victim].state = State::Waiting {
                            ready_at: now,
                            attempt,
                        };
                        self.entries[victim].resume_remaining = Some(remaining);
                    }

                    let (attempt, remaining) = match self.entries[candidate].state {
                        State::Waiting { attempt, .. } => {
                            if let Some(rem) = self.entries[candidate].resume_remaining.take() {
                                (attempt, rem)
                            } else {
                                (attempt + 1, self.entries[candidate].job.duration)
                            }
                        }
                        _ => continue,
                    };
                    let finish = now.saturating_add(remaining);
                    self.entries[candidate].state = State::Running {
                        attempt,
                        finish_at: finish,
                    };
                    self.entries[candidate].started_at = now;
                    trace.push(format!(
                        "{} start {} {} {}",
                        now, self.entries[candidate].job.id, attempt, remaining
                    ));
                    acted = true;
                    break;
                }
                if !acted {
                    break;
                }
            }

            if now == until {
                break;
            }
            let Some(next) = self.next_event_after(now) else {
                break;
            };
            if next > until {
                break;
            }
            now = next;
        }

        self.horizon = until;
        self.has_run = true;
        Ok(trace)
    }
}

fn main() {}