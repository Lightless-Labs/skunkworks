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

struct JobSpec {
    id: String,
    priority: u8,
    dependencies: Vec<usize>,
    max_attempts: u8,
}

struct JobState {
    state: State,
    attempts: u8,
}

pub struct Runner {
    jobs: Vec<JobSpec>,
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
                && job.id.bytes().all(|b| {
                    b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-'
                });
            if !valid_id {
                return Err(Error::BadId(index));
            }
            if job.priority > 9 {
                return Err(Error::BadPriority(index));
            }
            if !(1..=32).contains(&job.max_attempts) {
                return Err(Error::BadAttempts(index));
            }
            if ids.insert(job.id.clone(), index).is_some() {
                return Err(Error::Duplicate(index));
            }
        }

        let mut specs = Vec::with_capacity(jobs.len());
        for (job_index, job) in jobs.iter().enumerate() {
            let mut seen = std::collections::HashSet::new();
            let mut dependencies = Vec::with_capacity(job.dependencies.len());
            for (dep_index, dependency) in job.dependencies.iter().enumerate() {
                if !seen.insert(dependency) {
                    return Err(Error::DuplicateDependency(job_index, dep_index));
                }
                match ids.get(dependency) {
                    Some(&index) => dependencies.push(index),
                    None => return Err(Error::MissingDependency(job_index, dep_index)),
                }
            }
            specs.push(JobSpec {
                id: job.id.clone(),
                priority: job.priority,
                dependencies,
                max_attempts: job.max_attempts,
            });
        }

        if Self::has_cycle(&specs) {
            return Err(Error::Cycle);
        }

        let states = specs
            .iter()
            .map(|_| JobState {
                state: State::Waiting { ready_at: 0 },
                attempts: 0,
            })
            .collect();

        Ok(Self {
            jobs: specs,
            states,
            aging_ms,
            retry_base,
            retry_cap,
            last_time: 0,
            cycle_started: false,
        })
    }

    fn has_cycle(jobs: &[JobSpec]) -> bool {
        fn visit(index: usize, jobs: &[JobSpec], marks: &mut [u8]) -> bool {
            if marks[index] == 1 {
                return true;
            }
            if marks[index] == 2 {
                return false;
            }
            marks[index] = 1;
            for &dependency in &jobs[index].dependencies {
                if visit(dependency, jobs, marks) {
                    return true;
                }
            }
            marks[index] = 2;
            false
        }

        let mut marks = vec![0; jobs.len()];
        (0..jobs.len()).any(|index| visit(index, jobs, &mut marks))
    }

    fn check_time(&self, now: u64) -> Result<(), Error> {
        if now < self.last_time {
            Err(Error::TimeReversed)
        } else {
            Ok(())
        }
    }

    fn record_time(&mut self, now: u64) {
        self.last_time = now;
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
        self.record_time(now);
        Ok(())
    }

    pub fn next_ready(&mut self, now: u64) -> Result<Option<Ticket>, Error> {
        self.check_time(now)?;
        if !self.cycle_started {
            return Err(Error::NoCycle);
        }

        let mut selected: Option<(usize, u8)> = None;
        for (index, spec) in self.jobs.iter().enumerate() {
            let ready_at = match self.states[index].state {
                State::Waiting { ready_at } if ready_at <= now => ready_at,
                _ => continue,
            };
            if !spec
                .dependencies
                .iter()
                .all(|&dep| matches!(self.states[dep].state, State::Succeeded))
            {
                continue;
            }

            let effective = if self.aging_ms == 0 {
                spec.priority
            } else {
                let age = (now - ready_at) / self.aging_ms;
                (spec.priority as u64 + age).min(9) as u8
            };

            if selected.map_or(true, |(_, best)| effective > best) {
                selected = Some((index, effective));
            }
        }

        self.record_time(now);
        if let Some((index, _)) = selected {
            let state = &mut self.states[index];
            state.attempts += 1;
            state.state = State::Running {
                attempt: state.attempts,
            };
            Ok(Some(Ticket {
                id: self.jobs[index].id.clone(),
                attempt: state.attempts,
            }))
        } else {
            Ok(None)
        }
    }

    pub fn complete(
        &mut self,
        now: u64,
        id: &str,
        success: bool,
    ) -> Result<(), Error> {
        self.check_time(now)?;
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
        } else if self.states[index].attempts == self.jobs[index].max_attempts {
            self.states[index].state = State::Failed;
            self.propagate_blocked();
        } else {
            let attempt = self.states[index].attempts;
            let delay = self.retry_delay(attempt);
            self.states[index].state = State::Waiting {
                ready_at: now.saturating_add(delay),
            };
        }

        self.record_time(now);
        Ok(())
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
            let mut changed = false;
            for index in 0..self.jobs.len() {
                if !matches!(self.states[index].state, State::Waiting { .. }) {
                    continue;
                }
                if self.jobs[index].dependencies.iter().any(|&dep| {
                    matches!(self.states[dep].state, State::Failed | State::Blocked)
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
}