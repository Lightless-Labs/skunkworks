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

struct JobRecord {
    job: Job,
    dependency_indices: Vec<usize>,
    attempts: u8,
    state: Option<State>,
}

pub struct Runner {
    jobs: Vec<JobRecord>,
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
            let mut dependencies = Vec::with_capacity(job.dependencies.len());
            for (dependency_index, dependency) in job.dependencies.iter().enumerate() {
                if !seen.insert(dependency.as_str()) {
                    return Err(Error::DuplicateDependency(job_index, dependency_index));
                }
                match ids.get(dependency.as_str()) {
                    Some(&index) => dependencies.push(index),
                    None => return Err(Error::MissingDependency(job_index, dependency_index)),
                }
            }
            dependency_indices.push(dependencies);
        }

        if graph_has_cycle(&dependency_indices) {
            return Err(Error::Cycle);
        }

        let records = jobs
            .into_iter()
            .zip(dependency_indices)
            .map(|(job, dependency_indices)| JobRecord {
                job,
                dependency_indices,
                attempts: 0,
                state: None,
            })
            .collect();

        Ok(Self {
            jobs: records,
            aging_ms,
            retry_base,
            retry_cap,
            last_time: 0,
            cycle_started: false,
        })
    }

    pub fn begin_cycle(&mut self, now: u64) -> Result<(), Error> {
        self.check_time(now)?;
        if self.jobs.iter().any(|record| {
            matches!(
                record.state,
                Some(State::Waiting { .. } | State::Running { .. })
            )
        }) {
            return Err(Error::Busy);
        }

        for record in &mut self.jobs {
            record.attempts = 0;
            record.state = Some(State::Waiting { ready_at: now });
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

        let mut selected: Option<(usize, u8)> = None;
        for index in 0..self.jobs.len() {
            let record = &self.jobs[index];
            let ready_at = match record.state {
                Some(State::Waiting { ready_at }) if ready_at <= now => ready_at,
                _ => continue,
            };

            if !record
                .dependency_indices
                .iter()
                .all(|&dependency| matches!(self.jobs[dependency].state, Some(State::Succeeded)))
            {
                continue;
            }

            let effective_priority = if self.aging_ms == 0 {
                record.job.priority
            } else {
                let age = (now - ready_at) / self.aging_ms;
                let increased = u64::from(record.job.priority).saturating_add(age);
                increased.min(9) as u8
            };

            if selected
                .map(|(_, priority)| effective_priority > priority)
                .unwrap_or(true)
            {
                selected = Some((index, effective_priority));
            }
        }

        let result = if let Some((index, _)) = selected {
            let record = &mut self.jobs[index];
            record.attempts += 1;
            record.state = Some(State::Running {
                attempt: record.attempts,
            });
            Some(Ticket {
                id: record.job.id.clone(),
                attempt: record.attempts,
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

        let index = match self.jobs.iter().position(|record| record.job.id == id) {
            Some(index) => index,
            None => return Err(Error::Unknown),
        };

        if !matches!(self.jobs[index].state, Some(State::Running { .. })) {
            return Err(Error::NotRunning);
        }

        let new_state = if success {
            State::Succeeded
        } else if self.jobs[index].attempts == self.jobs[index].job.max_attempts {
            State::Failed
        } else {
            let delay = retry_delay(
                self.jobs[index].attempts,
                self.retry_base,
                self.retry_cap,
            );
            State::Waiting {
                ready_at: now.saturating_add(delay),
            }
        };

        let terminal_failure = matches!(new_state, State::Failed);
        self.jobs[index].state = Some(new_state);

        if terminal_failure {
            self.propagate_blocked();
        }

        self.last_time = now;
        Ok(())
    }

    pub fn status(&self, id: &str) -> Result<State, Error> {
        if !self.cycle_started {
            return Err(Error::NoCycle);
        }
        let record = self
            .jobs
            .iter()
            .find(|record| record.job.id == id)
            .ok_or(Error::Unknown)?;
        record.state.clone().ok_or(Error::NoCycle)
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
            let mut newly_blocked = Vec::new();
            for (index, record) in self.jobs.iter().enumerate() {
                if matches!(record.state, Some(State::Waiting { .. }))
                    && record.dependency_indices.iter().any(|&dependency| {
                        matches!(
                            self.jobs[dependency].state,
                            Some(State::Failed | State::Blocked)
                        )
                    })
                {
                    newly_blocked.push(index);
                }
            }

            if newly_blocked.is_empty() {
                break;
            }

            for index in newly_blocked {
                self.jobs[index].state = Some(State::Blocked);
            }
        }
    }
}

fn valid_id(id: &str) -> bool {
    (1..=16).contains(&id.len())
        && id
            .bytes()
            .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'-')
}

fn graph_has_cycle(dependencies: &[Vec<usize>]) -> bool {
    fn visit(node: usize, dependencies: &[Vec<usize>], marks: &mut [u8]) -> bool {
        match marks[node] {
            1 => return true,
            2 => return false,
            _ => {}
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

fn retry_delay(attempt: u8, retry_base: u64, retry_cap: u64) -> u64 {
    let mut delay = retry_base;
    for _ in 1..attempt {
        if delay >= retry_cap || delay > retry_cap / 2 {
            return retry_cap;
        }
        delay *= 2;
    }
    delay.min(retry_cap)
}