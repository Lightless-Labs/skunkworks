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
    dependency_indices: Vec<Vec<usize>>,
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

        let mut dependency_indices = Vec::with_capacity(jobs.len());
        for (job_index, job) in jobs.iter().enumerate() {
            let mut seen = std::collections::HashSet::new();
            let mut indices = Vec::with_capacity(job.dependencies.len());
            for (dependency_index, dependency) in job.dependencies.iter().enumerate() {
                if !seen.insert(dependency.as_str()) {
                    return Err(Error::DuplicateDependency(job_index, dependency_index));
                }
                let Some(&index) = ids.get(dependency.as_str()) else {
                    return Err(Error::MissingDependency(job_index, dependency_index));
                };
                indices.push(index);
            }
            dependency_indices.push(indices);
        }

        if has_cycle(&dependency_indices) {
            return Err(Error::Cycle);
        }

        let states = (0..jobs.len())
            .map(|_| JobState {
                state: State::Waiting { ready_at: 0 },
                attempts: 0,
            })
            .collect();

        Ok(Self {
            jobs,
            dependency_indices,
            states,
            aging_ms,
            retry_base,
            retry_cap,
            last_time: 0,
            cycle_started: false,
        })
    }

    pub fn begin_cycle(&mut self, now: u64) -> Result<(), Error> {
        self.check_time(now)?;
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
        self.check_time(now)?;
        if !self.cycle_started {
            return Err(Error::NoCycle);
        }

        let mut best: Option<(usize, u8)> = None;
        for index in 0..self.jobs.len() {
            let State::Waiting { ready_at } = self.states[index].state else {
                continue;
            };
            if ready_at > now
                || self.dependency_indices[index]
                    .iter()
                    .any(|&dependency| !matches!(self.states[dependency].state, State::Succeeded))
            {
                continue;
            }

            let effective = if self.aging_ms == 0 {
                self.jobs[index].priority
            } else {
                let aged = (now - ready_at) / self.aging_ms;
                (u64::from(self.jobs[index].priority) + aged).min(9) as u8
            };

            if best.map_or(true, |(best_index, best_priority)| {
                effective > best_priority || (effective == best_priority && index < best_index)
            }) {
                best = Some((index, effective));
            }
        }

        let result = if let Some((index, _)) = best {
            let attempt = self.states[index].attempts + 1;
            self.states[index].attempts = attempt;
            self.states[index].state = State::Running { attempt };
            Some(Ticket {
                id: self.jobs[index].id.clone(),
                attempt,
            })
        } else {
            None
        };

        self.last_time = now;
        Ok(result)
    }

    pub fn complete(&mut self, now: u64, id: &str, success: bool) -> Result<(), Error> {
        self.check_time(now)?;
        if !self.cycle_started {
            return Err(Error::NoCycle);
        }

        let Some(index) = self.jobs.iter().position(|job| job.id == id) else {
            return Err(Error::Unknown);
        };
        if !matches!(self.states[index].state, State::Running { .. }) {
            return Err(Error::NotRunning);
        }

        let attempt = self.states[index].attempts;
        if success {
            self.states[index].state = State::Succeeded;
        } else if attempt == self.jobs[index].max_attempts {
            self.states[index].state = State::Failed;
        } else {
            let delay = self.retry_delay(attempt);
            self.states[index].state = State::Waiting {
                ready_at: now.saturating_add(delay),
            };
        }

        if !success && matches!(self.states[index].state, State::Failed) {
            self.propagate_blocked();
        }

        self.last_time = now;
        Ok(())
    }

    pub fn status(&self, id: &str) -> Result<State, Error> {
        if !self.cycle_started {
            return Err(Error::NoCycle);
        }
        let Some(index) = self.jobs.iter().position(|job| job.id == id) else {
            return Err(Error::Unknown);
        };
        Ok(self.states[index].state.clone())
    }

    fn check_time(&self, now: u64) -> Result<(), Error> {
        if now < self.last_time {
            Err(Error::TimeReversed)
        } else {
            Ok(())
        }
    }

    fn retry_delay(&self, attempt: u8) -> u64 {
        let mut delay = self.retry_base;
        for _ in 1..attempt {
            if delay >= self.retry_cap || delay > self.retry_cap / 2 {
                return self.retry_cap;
            }
            delay *= 2;
        }
        delay.min(self.retry_cap)
    }

    fn propagate_blocked(&mut self) {
        loop {
            let mut newly_blocked = Vec::new();
            for index in 0..self.jobs.len() {
                if !matches!(self.states[index].state, State::Waiting { .. }) {
                    continue;
                }
                if self.dependency_indices[index].iter().any(|&dependency| {
                    matches!(
                        self.states[dependency].state,
                        State::Failed | State::Blocked
                    )
                }) {
                    newly_blocked.push(index);
                }
            }
            if newly_blocked.is_empty() {
                break;
            }
            for index in newly_blocked {
                self.states[index].state = State::Blocked;
            }
        }
    }
}

fn valid_id(id: &str) -> bool {
    let bytes = id.as_bytes();
    (1..=16).contains(&bytes.len())
        && bytes
            .iter()
            .all(|&byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'-')
}

fn has_cycle(dependencies: &[Vec<usize>]) -> bool {
    fn visit(node: usize, dependencies: &[Vec<usize>], colors: &mut [u8]) -> bool {
        if colors[node] == 1 {
            return true;
        }
        if colors[node] == 2 {
            return false;
        }

        colors[node] = 1;
        for &dependency in &dependencies[node] {
            if visit(dependency, dependencies, colors) {
                return true;
            }
        }
        colors[node] = 2;
        false
    }

    let mut colors = vec![0; dependencies.len()];
    for node in 0..dependencies.len() {
        if visit(node, dependencies, &mut colors) {
            return true;
        }
    }
    false
}