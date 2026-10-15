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
    cycle_started: bool,
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
        for (index, job) in jobs.iter().enumerate() {
            let valid_id = (1..=16).contains(&job.id.len())
                && job
                    .id
                    .bytes()
                    .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-');
            if !valid_id {
                return Err(Error::BadId(index));
            }
            if job.priority > 9 {
                return Err(Error::BadPriority(index));
            }
            if !(1..=32).contains(&job.max_attempts) {
                return Err(Error::BadAttempts(index));
            }
            if ids.insert(job.id.as_str(), index).is_some() {
                return Err(Error::Duplicate(index));
            }
        }

        let mut dependencies = Vec::with_capacity(jobs.len());
        for (job_index, job) in jobs.iter().enumerate() {
            let mut seen = std::collections::HashSet::new();
            let mut resolved = Vec::with_capacity(job.dependencies.len());
            for (dependency_index, dependency) in job.dependencies.iter().enumerate() {
                if !seen.insert(dependency.as_str()) {
                    return Err(Error::DuplicateDependency(job_index, dependency_index));
                }
                match ids.get(dependency.as_str()) {
                    Some(&index) => resolved.push(index),
                    None => {
                        return Err(Error::MissingDependency(job_index, dependency_index));
                    }
                }
            }
            dependencies.push(resolved);
        }

        fn visit(
            node: usize,
            dependencies: &[Vec<usize>],
            marks: &mut [u8],
        ) -> bool {
            if marks[node] == 1 {
                return false;
            }
            if marks[node] == 2 {
                return true;
            }
            marks[node] = 1;
            for &dependency in &dependencies[node] {
                if !visit(dependency, dependencies, marks) {
                    return false;
                }
            }
            marks[node] = 2;
            true
        }

        let mut marks = vec![0; jobs.len()];
        for node in 0..jobs.len() {
            if !visit(node, &dependencies, &mut marks) {
                return Err(Error::Cycle);
            }
        }

        let states = jobs
            .iter()
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
            cycle_started: false,
        })
    }

    pub fn begin_cycle(&mut self, now: u64) -> Result<(), Error> {
        if now < self.last_time {
            return Err(Error::TimeReversed);
        }
        if self.cycle_started
            && self.states.iter().any(|job| {
                matches!(job.state, State::Waiting { .. } | State::Running { .. })
            })
        {
            return Err(Error::Busy);
        }

        for job in &mut self.states {
            job.state = State::Waiting { ready_at: now };
            job.attempts = 0;
        }
        self.cycle_started = true;
        self.last_time = now;
        Ok(())
    }

    pub fn next_ready(&mut self, now: u64) -> Result<Option<Ticket>, Error> {
        if now < self.last_time {
            return Err(Error::TimeReversed);
        }
        if !self.cycle_started {
            return Err(Error::NoCycle);
        }

        let mut best: Option<(usize, u8)> = None;
        for index in 0..self.jobs.len() {
            let ready_at = match self.states[index].state {
                State::Waiting { ready_at } if ready_at <= now => ready_at,
                _ => continue,
            };
            if !self.dependencies[index]
                .iter()
                .all(|&dependency| matches!(self.states[dependency].state, State::Succeeded))
            {
                continue;
            }

            let base = self.jobs[index].priority;
            let effective = if self.aging_ms == 0 {
                base
            } else {
                let gained = (now - ready_at) / self.aging_ms;
                (base as u64 + gained).min(9) as u8
            };
            if best.map_or(true, |(best_index, best_priority)| {
                effective > best_priority || (effective == best_priority && index < best_index)
            }) {
                best = Some((index, effective));
            }
        }

        let result = if let Some((index, _)) = best {
            let attempts = self.states[index].attempts.saturating_add(1);
            self.states[index].attempts = attempts;
            self.states[index].state = State::Running { attempt: attempts };
            Some(Ticket {
                id: self.jobs[index].id.clone(),
                attempt: attempts,
            })
        } else {
            None
        };
        self.last_time = now;
        Ok(result)
    }

    pub fn complete(&mut self, now: u64, id: &str, success: bool) -> Result<(), Error> {
        if now < self.last_time {
            return Err(Error::TimeReversed);
        }
        if !self.cycle_started {
            return Err(Error::NoCycle);
        }

        let index = self
            .jobs
            .iter()
            .position(|job| job.id == id)
            .ok_or(Error::Unknown)?;
        if !matches!(self.states[index].state, State::Running { .. }) {
            return Err(Error::NotRunning);
        }

        if success {
            self.states[index].state = State::Succeeded;
        } else {
            let attempt = self.states[index].attempts;
            if attempt == self.jobs[index].max_attempts {
                self.states[index].state = State::Failed;
                self.propagate_blocked();
            } else {
                let exponent = attempt.saturating_sub(1) as u32;
                let delay = self
                    .retry_base
                    .checked_mul(1u64.checked_shl(exponent).unwrap_or(u64::MAX))
                    .unwrap_or(u64::MAX)
                    .min(self.retry_cap);
                self.states[index].state = State::Waiting {
                    ready_at: now.saturating_add(delay),
                };
            }
        }

        self.last_time = now;
        Ok(())
    }

    pub fn status(&self, id: &str) -> Result<State, Error> {
        if !self.cycle_started {
            return Err(Error::NoCycle);
        }
        let index = self
            .jobs
            .iter()
            .position(|job| job.id == id)
            .ok_or(Error::Unknown)?;
        Ok(self.states[index].state.clone())
    }

    fn propagate_blocked(&mut self) {
        loop {
            let mut changed = false;
            for index in 0..self.jobs.len() {
                if !matches!(self.states[index].state, State::Waiting { .. }) {
                    continue;
                }
                if self.dependencies[index].iter().any(|&dependency| {
                    matches!(
                        self.states[dependency].state,
                        State::Failed | State::Blocked
                    )
                }) {
                    self.states[index].state = State::Blocked;
                    changed = true;
                }
            }
            if !changed {
                break;
            }
        }
    }
}

fn main() {}