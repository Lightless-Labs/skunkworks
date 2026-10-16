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

#[derive(Debug, Clone)]
enum InternalState {
    Waiting {
        ready_at: u64,
        attempt: u8,
        remaining: Option<u64>,
    },
    Running {
        attempt: u8,
        finish_at: u64,
        started_at: u64,
        remaining: u64,
    },
    Succeeded,
    Failed,
    Blocked,
}

pub struct Pool {
    jobs: Vec<Job>,
    states: Vec<InternalState>,
    dependencies: Vec<Vec<usize>>,
    horizon: u64,
    has_run: bool,
    capacity: usize,
    aging_ms: u64,
    retry_base: u64,
    retry_cap: u64,
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

        let mut ids = std::collections::HashMap::new();

        for (i, job) in jobs.iter().enumerate() {
            if !valid_name(&job.id, 1, 16) {
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
            if job
                .tag
                .as_ref()
                .is_some_and(|tag| !valid_name(tag, 1, 8))
            {
                return Err(Error::BadTag(i));
            }
            if ids.insert(job.id.as_str(), i).is_some() {
                return Err(Error::Duplicate(i));
            }
        }

        let mut dependencies = vec![Vec::new(); jobs.len()];
        for (i, job) in jobs.iter().enumerate() {
            let mut seen = std::collections::HashSet::new();
            for (j, name) in job.dependencies.iter().enumerate() {
                if !seen.insert(name.as_str()) {
                    return Err(Error::DuplicateDependency(i, j));
                }
                match ids.get(name.as_str()) {
                    Some(&index) => dependencies[i].push(index),
                    None => return Err(Error::MissingDependency(i, j)),
                }
            }
        }

        let mut marks = vec![0u8; jobs.len()];
        fn visit(i: usize, dependencies: &[Vec<usize>], marks: &mut [u8]) -> bool {
            if marks[i] == 1 {
                return false;
            }
            if marks[i] == 2 {
                return true;
            }
            marks[i] = 1;
            for &dep in &dependencies[i] {
                if !visit(dep, dependencies, marks) {
                    return false;
                }
            }
            marks[i] = 2;
            true
        }
        for i in 0..jobs.len() {
            if !visit(i, &dependencies, &mut marks) {
                return Err(Error::Cycle);
            }
        }

        let states = jobs
            .iter()
            .map(|job| InternalState::Waiting {
                ready_at: job.release_at,
                attempt: 0,
                remaining: None,
            })
            .collect();

        Ok(Self {
            jobs,
            states,
            dependencies,
            horizon: 0,
            has_run: false,
            capacity,
            aging_ms,
            retry_base,
            retry_cap,
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
        let mut current = if self.has_run {
            self.horizon
        } else {
            0
        };
        let mut first_phase = !self.has_run;

        loop {
            if !first_phase && current == self.horizon && current == until {
                break;
            }
            first_phase = false;

            self.process_timestamp(current, &mut lines);

            if current == until {
                break;
            }
            match self.next_event_after(current, until) {
                Some(next) => current = next,
                None => break,
            }
        }

        self.horizon = until;
        self.has_run = true;
        Ok(lines)
    }

    pub fn status(&self, id: &str) -> Result<State, Error> {
        let index = self
            .jobs
            .iter()
            .position(|job| job.id == id)
            .ok_or(Error::Unknown)?;
        Ok(match self.states[index] {
            InternalState::Waiting {
                ready_at, attempt, ..
            } => State::Waiting { ready_at, attempt },
            InternalState::Running {
                attempt, finish_at, ..
            } => State::Running { attempt, finish_at },
            InternalState::Succeeded => State::Succeeded,
            InternalState::Failed => State::Failed,
            InternalState::Blocked => State::Blocked,
        })
    }

    fn process_timestamp(&mut self, time: u64, lines: &mut Vec<String>) {
        let mut completed: Vec<usize> = self
            .states
            .iter()
            .enumerate()
            .filter_map(|(i, state)| match state {
                InternalState::Running { finish_at, .. } if *finish_at == time => Some(i),
                _ => None,
            })
            .collect();
        completed.sort_by(|&a, &b| self.jobs[a].id.cmp(&self.jobs[b].id));

        for i in completed {
            let attempt = match self.states[i] {
                InternalState::Running { attempt, .. } => attempt,
                _ => continue,
            };
            let ok = attempt > self.jobs[i].failures;
            if ok {
                lines.push(format!(
                    "{} finish {} {} ok",
                    time, self.jobs[i].id, attempt
                ));
                self.states[i] = InternalState::Succeeded;
            } else {
                lines.push(format!(
                    "{} finish {} {} fail",
                    time, self.jobs[i].id, attempt
                ));
                if attempt >= self.jobs[i].max_attempts {
                    self.states[i] = InternalState::Failed;
                } else {
                    let delay = retry_delay(self.retry_base, self.retry_cap, attempt);
                    let ready_at = time.saturating_add(delay);
                    lines.push(format!(
                        "{} retry {} {} {}",
                        time, self.jobs[i].id, attempt, ready_at
                    ));
                    self.states[i] = InternalState::Waiting {
                        ready_at,
                        attempt,
                        remaining: None,
                    };
                }
            }
        }

        self.propagate_blocks(time, lines);
        self.schedule(time, lines);
    }

    fn propagate_blocks(&mut self, time: u64, lines: &mut Vec<String>) {
        let mut newly_blocked = Vec::new();
        loop {
            let mut changed = false;
            for i in 0..self.jobs.len() {
                if !matches!(self.states[i], InternalState::Waiting { .. }) {
                    continue;
                }
                let failed_dep = self.dependencies[i].iter().find(|&&dep| {
                    matches!(
                        self.states[dep],
                        InternalState::Failed | InternalState::Blocked
                    )
                });
                if let Some(&dep) = failed_dep {
                    self.states[i] = InternalState::Blocked;
                    newly_blocked.push((i, dep));
                    changed = true;
                }
            }
            if !changed {
                break;
            }
        }

        newly_blocked.sort_by(|(a, _), (b, _)| self.jobs[*a].id.cmp(&self.jobs[*b].id));
        for (i, dep) in newly_blocked {
            lines.push(format!(
                "{} skip {} {}",
                time, self.jobs[i].id, self.jobs[dep].id
            ));
        }
    }

    fn schedule(&mut self, time: u64, lines: &mut Vec<String>) {
        loop {
            let mut candidates: Vec<usize> = (0..self.jobs.len())
                .filter(|&i| {
                    matches!(
                        self.states[i],
                        InternalState::Waiting { ready_at, .. }
                            if ready_at <= time && self.dependencies[i].iter().all(|&dep| {
                                matches!(self.states[dep], InternalState::Succeeded)
                            })
                    )
                })
                .collect();

            candidates.sort_by(|&a, &b| {
                let pa = self.effective_priority(a, time);
                let pb = self.effective_priority(b, time);
                pb.cmp(&pa).then_with(|| a.cmp(&b))
            });

            let mut action = None;
            for candidate in candidates {
                let running: Vec<usize> = self
                    .states
                    .iter()
                    .enumerate()
                    .filter_map(|(i, state)| {
                        matches!(state, InternalState::Running { .. }).then_some(i)
                    })
                    .collect();

                let tag_free = self.jobs[candidate].tag.as_ref().is_none_or(|tag| {
                    !running
                        .iter()
                        .any(|&i| self.jobs[i].tag.as_ref() == Some(tag))
                });
                if running.len() < self.capacity && tag_free {
                    action = Some((candidate, None));
                    break;
                }

                let mut victims: Vec<usize> = running
                    .iter()
                    .copied()
                    .filter(|&victim| {
                        self.jobs[candidate].priority
                            >= self.jobs[victim].priority.saturating_add(2)
                    })
                    .filter(|&victim| {
                        let remaining_running: Vec<usize> = running
                            .iter()
                            .copied()
                            .filter(|&i| i != victim)
                            .collect();
                        remaining_running.len() < self.capacity
                            && self.jobs[candidate].tag.as_ref().is_none_or(|tag| {
                                !remaining_running
                                    .iter()
                                    .any(|&i| self.jobs[i].tag.as_ref() == Some(tag))
                            })
                    })
                    .collect();

                victims.sort_by(|&a, &b| {
                    self.jobs[a]
                        .priority
                        .cmp(&self.jobs[b].priority)
                        .then_with(|| {
                            let sa = match self.states[a] {
                                InternalState::Running { started_at, .. } => started_at,
                                _ => 0,
                            };
                            let sb = match self.states[b] {
                                InternalState::Running { started_at, .. } => started_at,
                                _ => 0,
                            };
                            sb.cmp(&sa)
                        })
                        .then_with(|| self.jobs[b].id.cmp(&self.jobs[a].id))
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
                let remaining = match self.states[victim] {
                    InternalState::Running { finish_at, .. } => finish_at - time,
                    _ => 0,
                };
                lines.push(format!(
                    "{} preempt {} by {}",
                    time, self.jobs[victim].id, self.jobs[candidate].id
                ));
                let attempt = match self.states[victim] {
                    InternalState::Running { attempt, .. } => attempt,
                    _ => 0,
                };
                self.states[victim] = InternalState::Waiting {
                    ready_at: time,
                    attempt,
                    remaining: Some(remaining),
                };
            }

            let (attempt, remaining) = match self.states[candidate].clone() {
                InternalState::Waiting {
                    attempt,
                    remaining: Some(remaining),
                    ..
                } => (attempt, remaining),
                InternalState::Waiting { attempt, .. } => {
                    (attempt.saturating_add(1), self.jobs[candidate].duration)
                }
                _ => continue,
            };
            let finish_at = time.saturating_add(remaining);
            lines.push(format!(
                "{} start {} {} {}",
                time, self.jobs[candidate].id, attempt, remaining
            ));
            self.states[candidate] = InternalState::Running {
                attempt,
                finish_at,
                started_at: time,
                remaining,
            };
        }
    }

    fn effective_priority(&self, i: usize, time: u64) -> u8 {
        if self.aging_ms == 0 {
            return self.jobs[i].priority;
        }
        let ready_at = match self.states[i] {
            InternalState::Waiting { ready_at, .. } => ready_at,
            _ => time,
        };
        let age = (time - ready_at) / self.aging_ms;
        self.jobs[i]
            .priority
            .saturating_add(age.min(9) as u8)
            .min(9)
    }

    fn next_event_after(&self, current: u64, until: u64) -> Option<u64> {
        let mut next: Option<u64> = None;
        for state in &self.states {
            let event = match state {
                InternalState::Waiting { ready_at, .. } if *ready_at > current => Some(*ready_at),
                InternalState::Running { finish_at, .. } if *finish_at > current => {
                    Some(*finish_at)
                }
                _ => None,
            };
            if let Some(event) = event.filter(|&t| t <= until) {
                next = Some(next.map_or(event, |old| old.min(event)));
            }
        }
        next
    }
}

fn valid_name(value: &str, min_len: usize, max_len: usize) -> bool {
    let bytes = value.as_bytes();
    (min_len..=max_len).contains(&bytes.len())
        && bytes
            .iter()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || *b == b'-')
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