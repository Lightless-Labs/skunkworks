#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Job {
    pub id: String,
    pub priority: u8,
    pub dependencies: Vec<String>,
    pub max_attempts: u8,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum State {
    Waiting { ready_at: u64 },
    Running { attempt: u8 },
    Succeeded,
    Failed,
    Blocked,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Ticket {
    pub id: String,
    pub attempt: u8,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Error {
    Config,
    BadId(usize),
    BadPriority(usize),
    BadAttempts(usize),
    Duplicate(usize),
    DuplicateDependency(usize, usize),
    MissingDependency(usize, usize),
    Cycle,
    TimeReversed,
    Busy,
    NoCycle,
    Unknown,
    NotRunning,
}

struct JobState {
    state: State,
    attempts: u8,
}

pub struct Runner {
    jobs: Vec<Job>,
    dependencies: Vec<Vec<usize>>,
    states: Vec<JobState>,
    aging_ms: u64,
    retry_base: u64,
    retry_cap: u64,
    last_time: u64,
    has_cycle: bool,
}

impl Runner {
    pub fn new(
        jobs: Vec<Job>,
        aging_ms: u64,
        retry_base: u64,
        retry_cap: u64,
    ) -> Result<Self, Error> {
        if retry_base == 0 || retry_cap < retry_base {
            return Err(Error::Config);
        }

        let mut ids = std::collections::HashMap::new();

        for (i, job) in jobs.iter().enumerate() {
            if !valid_id(&job.id) {
                return Err(Error::BadId(i));
            }
            if job.priority > 9 {
                return Err(Error::BadPriority(i));
            }
            if !(1..=32).contains(&job.max_attempts) {
                return Err(Error::BadAttempts(i));
            }
            if ids.insert(job.id.as_str(), i).is_some() {
                return Err(Error::Duplicate(i));
            }
        }

        let mut dependencies = Vec::with_capacity(jobs.len());
        for (i, job) in jobs.iter().enumerate() {
            let mut seen = std::collections::HashSet::new();
            let mut deps = Vec::with_capacity(job.dependencies.len());
            for (j, dep) in job.dependencies.iter().enumerate() {
                if !seen.insert(dep.as_str()) {
                    return Err(Error::DuplicateDependency(i, j));
                }
                match ids.get(dep.as_str()) {
                    Some(&index) => deps.push(index),
                    None => return Err(Error::MissingDependency(i, j)),
                }
            }
            dependencies.push(deps);
        }

        let mut marks = vec![0u8; jobs.len()];
        for i in 0..jobs.len() {
            if has_cycle_from(i, &dependencies, &mut marks) {
                return Err(Error::Cycle);
            }
        }

        let states = (0..jobs.len())
            .map(|_| JobState {
                state: State::Waiting { ready_at: 0 },
                attempts: 0,
            })
            .collect();

        Ok(Self {
            jobs,
            dependencies,
            states,
            aging_ms,
            retry_base,
            retry_cap,
            last_time: 0,
            has_cycle: false,
        })
    }

    pub fn begin_cycle(&mut self, now: u64) -> Result<(), Error> {
        self.check_time(now)?;
        if self.has_cycle
            && self.states.iter().any(|s| {
                matches!(s.state, State::Waiting { .. } | State::Running { .. })
            })
        {
            return Err(Error::Busy);
        }

        for state in &mut self.states {
            state.state = State::Waiting { ready_at: now };
            state.attempts = 0;
        }
        self.last_time = now;
        self.has_cycle = true;
        Ok(())
    }

    pub fn next_ready(&mut self, now: u64) -> Result<Option<Ticket>, Error> {
        self.check_time(now)?;
        if !self.has_cycle {
            return Err(Error::NoCycle);
        }

        let mut best: Option<(usize, u8)> = None;
        for i in 0..self.jobs.len() {
            let ready_at = match self.states[i].state {
                State::Waiting { ready_at } if ready_at <= now => ready_at,
                _ => continue,
            };
            if !self.dependencies[i]
                .iter()
                .all(|&d| matches!(self.states[d].state, State::Succeeded))
            {
                continue;
            }

            let priority = effective_priority(
                self.jobs[i].priority,
                ready_at,
                now,
                self.aging_ms,
            );
            if best.map_or(true, |(best_i, best_priority)| {
                priority > best_priority || (priority == best_priority && i < best_i)
            }) {
                best = Some((i, priority));
            }
        }

        self.last_time = now;
        let Some((i, _)) = best else {
            return Ok(None);
        };

        self.states[i].attempts += 1;
        let attempt = self.states[i].attempts;
        self.states[i].state = State::Running { attempt };
        Ok(Some(Ticket {
            id: self.jobs[i].id.clone(),
            attempt,
        }))
    }

    pub fn complete(&mut self, now: u64, id: &str, success: bool) -> Result<(), Error> {
        self.check_time(now)?;
        if !self.has_cycle {
            return Err(Error::NoCycle);
        }
        let i = self
            .jobs
            .iter()
            .position(|job| job.id == id)
            .ok_or(Error::Unknown)?;

        let attempt = match self.states[i].state {
            State::Running { attempt } => attempt,
            _ => return Err(Error::NotRunning),
        };

        self.last_time = now;
        if success {
            self.states[i].state = State::Succeeded;
        } else if attempt == self.jobs[i].max_attempts {
            self.states[i].state = State::Failed;
            self.propagate_blocked();
        } else {
            let delay = retry_delay(self.retry_base, self.retry_cap, attempt);
            self.states[i].state = State::Waiting {
                ready_at: now.saturating_add(delay),
            };
        }
        Ok(())
    }

    pub fn status(&self, id: &str) -> Result<State, Error> {
        if !self.has_cycle {
            return Err(Error::NoCycle);
        }
        let i = self
            .jobs
            .iter()
            .position(|job| job.id == id)
            .ok_or(Error::Unknown)?;
        Ok(self.states[i].state.clone())
    }

    fn check_time(&self, now: u64) -> Result<(), Error> {
        if now < self.last_time {
            Err(Error::TimeReversed)
        } else {
            Ok(())
        }
    }

    fn propagate_blocked(&mut self) {
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
                    changed = true;
                }
            }
            if !changed {
                break;
            }
        }
    }
}

fn valid_id(id: &str) -> bool {
    (1..=16).contains(&id.len())
        && id
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
}

fn has_cycle_from(node: usize, deps: &[Vec<usize>], marks: &mut [u8]) -> bool {
    if marks[node] == 1 {
        return true;
    }
    if marks[node] == 2 {
        return false;
    }
    marks[node] = 1;
    for &dep in &deps[node] {
        if has_cycle_from(dep, deps, marks) {
            return true;
        }
    }
    marks[node] = 2;
    false
}

fn effective_priority(base: u8, ready_at: u64, now: u64, aging_ms: u64) -> u8 {
    if aging_ms == 0 {
        return base;
    }
    let increments = (now - ready_at) / aging_ms;
    (base as u64 + increments).min(9) as u8
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