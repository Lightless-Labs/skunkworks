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
                let Some(&target) = ids.get(dependency.as_str()) else {
                    return Err(Error::MissingDependency(job_index, dependency_index));
                };
                resolved.push(target);
            }
            dependencies.push(resolved);
        }

        fn has_cycle(dependencies: &[Vec<usize>]) -> bool {
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
            (0..dependencies.len()).any(|node| visit(node, dependencies, &mut marks))
        }

        if has_cycle(&dependencies) {
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
        self.last_time = now;
        self.cycle_started = true;
        Ok(())
    }

    pub fn next_ready(&mut self, now: u64) -> Result<Option<Ticket>, Error> {
        if now < self.last_time {
            return Err(Error::TimeReversed);
        }
        if !self.cycle_started {
            return Err(Error::NoCycle);
        }

        let mut best: Option<(usize, u16)> = None;
        for index in 0..self.jobs.len() {
            let State::Waiting { ready_at } = self.states[index].state else {
                continue;
            };
            if ready_at > now
                || !self.dependencies[index]
                    .iter()
                    .all(|&dependency| matches!(self.states[dependency].state, State::Succeeded))
            {
                continue;
            }

            let age = if self.aging_ms == 0 {
                0
            } else {
                ((now - ready_at) / self.aging_ms).min(9) as u16
            };
            let effective = (self.jobs[index].priority as u16 + age).min(9);

            if best.is_none_or(|(best_index, best_priority)| {
                effective > best_priority || (effective == best_priority && index < best_index)
            }) {
                best = Some((index, effective));
            }
        }

        let result = if let Some((index, _)) = best {
            self.states[index].attempts += 1;
            let attempt = self.states[index].attempts;
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
        if now < self.last_time {
            return Err(Error::TimeReversed);
        }
        if !self.cycle_started {
            return Err(Error::NoCycle);
        }

        let Some(index) = self.jobs.iter().position(|job| job.id == id) else {
            return Err(Error::Unknown);
        };
        let State::Running { .. } = self.states[index].state else {
            return Err(Error::NotRunning);
        };

        if success {
            self.states[index].state = State::Succeeded;
        } else if self.states[index].attempts == self.jobs[index].max_attempts {
            self.states[index].state = State::Failed;
        } else {
            let attempt = self.states[index].attempts;
            let shift = u32::from(attempt.saturating_sub(1));
            let delay = self
                .retry_base
                .checked_mul(1u64.checked_shl(shift).unwrap_or(u64::MAX))
                .unwrap_or(u64::MAX)
                .min(self.retry_cap);
            self.states[index].state = State::Waiting {
                ready_at: now.saturating_add(delay),
            };
        }

        if matches!(self.states[index].state, State::Failed) {
            loop {
                let mut changed = false;
                for job_index in 0..self.jobs.len() {
                    if !matches!(self.states[job_index].state, State::Waiting { .. }) {
                        continue;
                    }
                    if self.dependencies[job_index].iter().any(|&dependency| {
                        matches!(
                            self.states[dependency].state,
                            State::Failed | State::Blocked
                        )
                    }) {
                        self.states[job_index].state = State::Blocked;
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
        let Some(index) = self.jobs.iter().position(|job| job.id == id) else {
            return Err(Error::Unknown);
        };
        Ok(self.states[index].state.clone())
    }
}