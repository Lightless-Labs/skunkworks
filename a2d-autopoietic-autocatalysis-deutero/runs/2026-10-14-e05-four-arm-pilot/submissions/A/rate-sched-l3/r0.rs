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

struct JobEntry {
    job: Job,
    dependencies: Vec<usize>,
    state: State,
    attempts: u8,
}

pub struct Runner {
    jobs: Vec<JobEntry>,
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
            if ids.insert(job.id.as_str(), i).is_some() {
                return Err(Error::Duplicate(i));
            }
        }

        let mut dependencies = Vec::with_capacity(jobs.len());
        for (i, job) in jobs.iter().enumerate() {
            let mut seen = std::collections::HashSet::new();
            let mut resolved = Vec::with_capacity(job.dependencies.len());
            for (j, dep) in job.dependencies.iter().enumerate() {
                if !seen.insert(dep.as_str()) {
                    return Err(Error::DuplicateDependency(i, j));
                }
                match ids.get(dep.as_str()) {
                    Some(&index) => resolved.push(index),
                    None => return Err(Error::MissingDependency(i, j)),
                }
            }
            dependencies.push(resolved);
        }

        fn has_cycle(
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
            for &dep in &dependencies[node] {
                if has_cycle(dep, dependencies, marks) {
                    return true;
                }
            }
            marks[node] = 2;
            false
        }

        let mut marks = vec![0; jobs.len()];
        for i in 0..jobs.len() {
            if has_cycle(i, &dependencies, &mut marks) {
                return Err(Error::Cycle);
            }
        }

        let entries = jobs
            .into_iter()
            .zip(dependencies)
            .map(|(job, dependencies)| JobEntry {
                job,
                dependencies,
                state: State::Waiting { ready_at: 0 },
                attempts: 0,
            })
            .collect();

        Ok(Self {
            jobs: entries,
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
            && self.jobs.iter().any(|entry| {
                matches!(entry.state, State::Waiting { .. } | State::Running { .. })
            })
        {
            return Err(Error::Busy);
        }

        for entry in &mut self.jobs {
            entry.state = State::Waiting { ready_at: now };
            entry.attempts = 0;
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
        for (i, entry) in self.jobs.iter().enumerate() {
            let ready_at = match entry.state {
                State::Waiting { ready_at } if ready_at <= now => ready_at,
                _ => continue,
            };
            if !entry
                .dependencies
                .iter()
                .all(|&dep| matches!(self.jobs[dep].state, State::Succeeded))
            {
                continue;
            }

            let priority = if self.aging_ms == 0 {
                entry.job.priority
            } else {
                let age = (now - ready_at) / self.aging_ms;
                (entry.job.priority as u64 + age).min(9) as u8
            };

            if best.is_none_or(|(_, best_priority)| priority > best_priority) {
                best = Some((i, priority));
            }
        }

        self.last_time = now;
        let Some((index, _)) = best else {
            return Ok(None);
        };

        let entry = &mut self.jobs[index];
        entry.attempts += 1;
        entry.state = State::Running {
            attempt: entry.attempts,
        };
        Ok(Some(Ticket {
            id: entry.job.id.clone(),
            attempt: entry.attempts,
        }))
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
            .position(|entry| entry.job.id == id)
            .ok_or(Error::Unknown)?;

        if !matches!(self.jobs[index].state, State::Running { .. }) {
            return Err(Error::NotRunning);
        }

        let entry = &mut self.jobs[index];
        if success {
            entry.state = State::Succeeded;
        } else if entry.attempts == entry.job.max_attempts {
            entry.state = State::Failed;
        } else {
            let exponent = entry.attempts.saturating_sub(1) as u32;
            let delay = self
                .retry_base
                .checked_mul(1u64.checked_shl(exponent).unwrap_or(u64::MAX))
                .unwrap_or(u64::MAX)
                .min(self.retry_cap);
            entry.state = State::Waiting {
                ready_at: now.saturating_add(delay),
            };
        }

        if matches!(self.jobs[index].state, State::Failed) {
            loop {
                let mut changed = false;
                for i in 0..self.jobs.len() {
                    if matches!(self.jobs[i].state, State::Waiting { .. })
                        && self.jobs[i].dependencies.iter().any(|&dep| {
                            matches!(
                                self.jobs[dep].state,
                                State::Failed | State::Blocked
                            )
                        })
                    {
                        self.jobs[i].state = State::Blocked;
                        changed = true;
                    }
                }
                if !changed {
                    break;
                }
            }
        }

        self.last_time = now;
        Ok(())
    }

    pub fn status(&self, id: &str) -> Result<State, Error> {
        if !self.cycle_started {
            return Err(Error::NoCycle);
        }
        self.jobs
            .iter()
            .find(|entry| entry.job.id == id)
            .map(|entry| entry.state.clone())
            .ok_or(Error::Unknown)
    }
}