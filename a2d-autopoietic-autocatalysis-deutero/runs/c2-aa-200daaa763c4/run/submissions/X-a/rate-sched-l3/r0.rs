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

struct JobData {
    job: Job,
    dependencies: Vec<usize>,
    state: State,
    attempts: u8,
}

pub struct Runner {
    jobs: Vec<JobData>,
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
            if ids.insert(job.id.clone(), i).is_some() {
                return Err(Error::Duplicate(i));
            }
        }

        let mut dependencies = Vec::with_capacity(jobs.len());
        for (i, job) in jobs.iter().enumerate() {
            let mut seen = std::collections::HashSet::new();
            let mut indices = Vec::with_capacity(job.dependencies.len());
            for (j, dep) in job.dependencies.iter().enumerate() {
                if !seen.insert(dep.as_str()) {
                    return Err(Error::DuplicateDependency(i, j));
                }
                let Some(&idx) = ids.get(dep) else {
                    return Err(Error::MissingDependency(i, j));
                };
                indices.push(idx);
            }
            dependencies.push(indices);
        }

        fn visit(node: usize, dependencies: &[Vec<usize>], marks: &mut [u8]) -> bool {
            if marks[node] == 1 {
                return true;
            }
            if marks[node] == 2 {
                return false;
            }
            marks[node] = 1;
            for &dep in &dependencies[node] {
                if visit(dep, dependencies, marks) {
                    return true;
                }
            }
            marks[node] = 2;
            false
        }

        let mut marks = vec![0; jobs.len()];
        for i in 0..jobs.len() {
            if visit(i, &dependencies, &mut marks) {
                return Err(Error::Cycle);
            }
        }

        let jobs = jobs
            .into_iter()
            .zip(dependencies)
            .map(|(job, dependencies)| JobData {
                job,
                dependencies,
                state: State::Waiting { ready_at: 0 },
                attempts: 0,
            })
            .collect();

        Ok(Self {
            jobs,
            aging_ms,
            retry_base,
            retry_cap,
            last_time: 0,
            cycle_started: false,
        })
    }

    fn check_time(&self, now: u64) -> Result<(), Error> {
        if now < self.last_time {
            Err(Error::TimeReversed)
        } else {
            Ok(())
        }
    }

    fn find_job(&self, id: &str) -> Option<usize> {
        self.jobs.iter().position(|entry| entry.job.id == id)
    }

    fn cycle_is_terminal(&self) -> bool {
        self.jobs.iter().all(|entry| {
            matches!(
                entry.state,
                State::Succeeded | State::Failed | State::Blocked
            )
        })
    }

    pub fn begin_cycle(&mut self, now: u64) -> Result<(), Error> {
        self.check_time(now)?;
        if self.cycle_started && !self.cycle_is_terminal() {
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
        self.check_time(now)?;
        if !self.cycle_started {
            return Err(Error::NoCycle);
        }

        let mut best: Option<(usize, u8)> = None;
        for (i, entry) in self.jobs.iter().enumerate() {
            let State::Waiting { ready_at } = entry.state else {
                continue;
            };
            if ready_at > now
                || !entry
                    .dependencies
                    .iter()
                    .all(|&dep| matches!(self.jobs[dep].state, State::Succeeded))
            {
                continue;
            }

            let effective = if self.aging_ms == 0 {
                entry.job.priority
            } else {
                let age = (now - ready_at) / self.aging_ms;
                (entry.job.priority as u64 + age).min(9) as u8
            };

            if best.is_none_or(|(best_i, best_priority)| {
                effective > best_priority || (effective == best_priority && i < best_i)
            }) {
                best = Some((i, effective));
            }
        }

        let result = if let Some((i, _)) = best {
            let entry = &mut self.jobs[i];
            entry.attempts += 1;
            entry.state = State::Running {
                attempt: entry.attempts,
            };
            Some(Ticket {
                id: entry.job.id.clone(),
                attempt: entry.attempts,
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
        let Some(i) = self.find_job(id) else {
            return Err(Error::Unknown);
        };
        if !matches!(self.jobs[i].state, State::Running { .. }) {
            return Err(Error::NotRunning);
        }

        if success {
            self.jobs[i].state = State::Succeeded;
        } else if self.jobs[i].attempts == self.jobs[i].job.max_attempts {
            self.jobs[i].state = State::Failed;
        } else {
            let mut delay = self.retry_base;
            for _ in 1..self.jobs[i].attempts {
                if delay >= self.retry_cap || delay > self.retry_cap / 2 {
                    delay = self.retry_cap;
                    break;
                }
                delay *= 2;
            }
            delay = delay.min(self.retry_cap);
            self.jobs[i].state = State::Waiting {
                ready_at: now.saturating_add(delay),
            };
        }

        if matches!(self.jobs[i].state, State::Failed) {
            loop {
                let newly_blocked: Vec<usize> = self
                    .jobs
                    .iter()
                    .enumerate()
                    .filter_map(|(index, entry)| {
                        if matches!(entry.state, State::Waiting { .. })
                            && entry.dependencies.iter().any(|&dep| {
                                matches!(
                                    self.jobs[dep].state,
                                    State::Failed | State::Blocked
                                )
                            })
                        {
                            Some(index)
                        } else {
                            None
                        }
                    })
                    .collect();

                if newly_blocked.is_empty() {
                    break;
                }
                for index in newly_blocked {
                    self.jobs[index].state = State::Blocked;
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
        let Some(i) = self.find_job(id) else {
            return Err(Error::Unknown);
        };
        Ok(self.jobs[i].state.clone())
    }
}