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

struct JobRuntime {
    state: State,
    attempts: u8,
}

pub struct Runner {
    jobs: Vec<Job>,
    dependency_indices: Vec<Vec<usize>>,
    runtime: Vec<JobRuntime>,
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

        for (index, job) in jobs.iter().enumerate() {
            if !valid_id(&job.id) {
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

        let mut dependency_indices = Vec::with_capacity(jobs.len());

        for (job_index, job) in jobs.iter().enumerate() {
            let mut seen = std::collections::HashSet::new();
            let mut indices = Vec::with_capacity(job.dependencies.len());

            for (dependency_index, dependency) in job.dependencies.iter().enumerate() {
                if !seen.insert(dependency.as_str()) {
                    return Err(Error::DuplicateDependency(job_index, dependency_index));
                }
                match ids.get(dependency.as_str()) {
                    Some(&index) => indices.push(index),
                    None => return Err(Error::MissingDependency(job_index, dependency_index)),
                }
            }

            dependency_indices.push(indices);
        }

        if graph_has_cycle(&dependency_indices) {
            return Err(Error::Cycle);
        }

        let runtime = (0..jobs.len())
            .map(|_| JobRuntime {
                state: State::Waiting { ready_at: 0 },
                attempts: 0,
            })
            .collect();

        Ok(Self {
            jobs,
            dependency_indices,
            runtime,
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
            && self.runtime.iter().any(|job| {
                matches!(job.state, State::Waiting { .. } | State::Running { .. })
            })
        {
            return Err(Error::Busy);
        }

        for job in &mut self.runtime {
            job.state = State::Waiting { ready_at: now };
            job.attempts = 0;
        }

        self.has_cycle = true;
        self.last_time = now;
        Ok(())
    }

    pub fn next_ready(&mut self, now: u64) -> Result<Option<Ticket>, Error> {
        self.check_time(now)?;
        if !self.has_cycle {
            return Err(Error::NoCycle);
        }

        let mut best: Option<(usize, u8)> = None;

        for index in 0..self.jobs.len() {
            let ready_at = match self.runtime[index].state {
                State::Waiting { ready_at } if ready_at <= now => ready_at,
                _ => continue,
            };

            if !self.dependency_indices[index]
                .iter()
                .all(|&dependency| matches!(self.runtime[dependency].state, State::Succeeded))
            {
                continue;
            }

            let effective_priority = if self.aging_ms == 0 {
                self.jobs[index].priority
            } else {
                let age_steps = (now - ready_at) / self.aging_ms;
                let priority = u64::from(self.jobs[index].priority).saturating_add(age_steps);
                priority.min(9) as u8
            };

            if best.map_or(true, |(_, best_priority)| effective_priority > best_priority) {
                best = Some((index, effective_priority));
            }
        }

        self.last_time = now;

        if let Some((index, _)) = best {
            let attempt = self.runtime[index].attempts.saturating_add(1);
            self.runtime[index].attempts = attempt;
            self.runtime[index].state = State::Running { attempt };
            Ok(Some(Ticket {
                id: self.jobs[index].id.clone(),
                attempt,
            }))
        } else {
            Ok(None)
        }
    }

    pub fn complete(&mut self, now: u64, id: &str, success: bool) -> Result<(), Error> {
        self.check_time(now)?;
        if !self.has_cycle {
            return Err(Error::NoCycle);
        }

        let index = self
            .jobs
            .iter()
            .position(|job| job.id == id)
            .ok_or(Error::Unknown)?;

        let attempt = match self.runtime[index].state {
            State::Running { attempt } => attempt,
            _ => return Err(Error::NotRunning),
        };

        if success {
            self.runtime[index].state = State::Succeeded;
        } else if attempt == self.jobs[index].max_attempts {
            self.runtime[index].state = State::Failed;
            self.propagate_blocked();
        } else {
            let delay = self.retry_delay(attempt);
            self.runtime[index].state = State::Waiting {
                ready_at: now.saturating_add(delay),
            };
        }

        self.last_time = now;
        Ok(())
    }

    pub fn status(&self, id: &str) -> Result<State, Error> {
        if !self.has_cycle {
            return Err(Error::NoCycle);
        }

        let index = self
            .jobs
            .iter()
            .position(|job| job.id == id)
            .ok_or(Error::Unknown)?;

        Ok(self.runtime[index].state.clone())
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
            if delay >= self.retry_cap {
                return self.retry_cap;
            }
            delay = delay.saturating_mul(2).min(self.retry_cap);
        }
        delay.min(self.retry_cap)
    }

    fn propagate_blocked(&mut self) {
        loop {
            let mut newly_blocked = Vec::new();

            for index in 0..self.jobs.len() {
                if !matches!(self.runtime[index].state, State::Waiting { .. }) {
                    continue;
                }

                if self.dependency_indices[index].iter().any(|&dependency| {
                    matches!(
                        self.runtime[dependency].state,
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
                self.runtime[index].state = State::Blocked;
            }
        }
    }
}

fn valid_id(id: &str) -> bool {
    let bytes = id.as_bytes();
    !bytes.is_empty()
        && bytes.len() <= 16
        && bytes
            .iter()
            .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || *byte == b'-')
}

fn graph_has_cycle(dependencies: &[Vec<usize>]) -> bool {
    fn visit(
        node: usize,
        dependencies: &[Vec<usize>],
        marks: &mut [u8],
    ) -> bool {
        if marks[node] == 1 {
            return true;
        }
        if marks[node] == 2 {
            return false;
        }

        marks[node] = 1;
        for &dependency in &dependencies[node] {
            if visit(dependency, dependencies, marks) {
                return true;
            }
        }
        marks[node] = 2;
        false
    }

    let mut marks = vec![0; dependencies.len()];
    for node in 0..dependencies.len() {
        if visit(node, dependencies, &mut marks) {
            return true;
        }
    }
    false
}