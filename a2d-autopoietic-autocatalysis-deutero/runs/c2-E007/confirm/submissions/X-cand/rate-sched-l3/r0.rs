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
    dependencies: Vec<usize>,
    state: State,
    attempts: u8,
}

pub struct Runner {
    jobs: Vec<JobRecord>,
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

        let mut resolved = Vec::with_capacity(jobs.len());
        for (i, job) in jobs.iter().enumerate() {
            let mut seen = std::collections::HashSet::new();
            let mut deps = Vec::with_capacity(job.dependencies.len());
            for (j, dep) in job.dependencies.iter().enumerate() {
                if !seen.insert(dep.as_str()) {
                    return Err(Error::DuplicateDependency(i, j));
                }
                match ids.get(dep) {
                    Some(&index) => deps.push(index),
                    None => return Err(Error::MissingDependency(i, j)),
                }
            }
            resolved.push(deps);
        }

        let mut indegree = vec![0usize; jobs.len()];
        let mut dependents = vec![Vec::new(); jobs.len()];
        for (i, deps) in resolved.iter().enumerate() {
            indegree[i] = deps.len();
            for &dep in deps {
                dependents[dep].push(i);
            }
        }
        let mut queue = std::collections::VecDeque::new();
        for (i, &degree) in indegree.iter().enumerate() {
            if degree == 0 {
                queue.push_back(i);
            }
        }
        let mut visited = 0usize;
        while let Some(node) = queue.pop_front() {
            visited += 1;
            for &dependent in &dependents[node] {
                indegree[dependent] -= 1;
                if indegree[dependent] == 0 {
                    queue.push_back(dependent);
                }
            }
        }
        if visited != jobs.len() {
            return Err(Error::Cycle);
        }

        let records = jobs
            .into_iter()
            .zip(resolved)
            .map(|(job, dependencies)| JobRecord {
                job,
                dependencies,
                state: State::Waiting { ready_at: 0 },
                attempts: 0,
            })
            .collect();

        Ok(Self {
            jobs: records,
            aging_ms,
            retry_base,
            retry_cap,
            last_time: 0,
            has_cycle: false,
        })
    }

    pub fn begin_cycle(&mut self, now: u64) -> Result<(), Error> {
        if now < self.last_time {
            return Err(Error::TimeReversed);
        }
        if self.has_cycle
            && self.jobs.iter().any(|job| {
                matches!(job.state, State::Waiting { .. } | State::Running { .. })
            })
        {
            return Err(Error::Busy);
        }

        for job in &mut self.jobs {
            job.state = State::Waiting { ready_at: now };
            job.attempts = 0;
        }
        self.last_time = now;
        self.has_cycle = true;
        Ok(())
    }

    pub fn next_ready(&mut self, now: u64) -> Result<Option<Ticket>, Error> {
        if now < self.last_time {
            return Err(Error::TimeReversed);
        }
        if !self.has_cycle {
            return Err(Error::NoCycle);
        }

        let mut best: Option<(usize, u8)> = None;
        for (i, job) in self.jobs.iter().enumerate() {
            let ready_at = match job.state {
                State::Waiting { ready_at } if ready_at <= now => ready_at,
                _ => continue,
            };
            if !job
                .dependencies
                .iter()
                .all(|&dep| matches!(self.jobs[dep].state, State::Succeeded))
            {
                continue;
            }

            let effective = if self.aging_ms == 0 {
                job.job.priority
            } else {
                let age = (now - ready_at) / self.aging_ms;
                let capped_age = age.min(9 - job.job.priority as u64);
                job.job.priority + capped_age as u8
            };

            if best.map_or(true, |(best_i, best_priority)| {
                effective > best_priority || (effective == best_priority && i < best_i)
            }) {
                best = Some((i, effective));
            }
        }

        let result = if let Some((i, _)) = best {
            let job = &mut self.jobs[i];
            job.attempts += 1;
            job.state = State::Running {
                attempt: job.attempts,
            };
            Some(Ticket {
                id: job.job.id.clone(),
                attempt: job.attempts,
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
        if !self.has_cycle {
            return Err(Error::NoCycle);
        }

        let index = self
            .jobs
            .iter()
            .position(|job| job.job.id == id)
            .ok_or(Error::Unknown)?;

        if !matches!(self.jobs[index].state, State::Running { .. }) {
            return Err(Error::NotRunning);
        }

        if success {
            self.jobs[index].state = State::Succeeded;
        } else if self.jobs[index].attempts == self.jobs[index].job.max_attempts {
            self.jobs[index].state = State::Failed;
            self.propagate_blocked();
        } else {
            let attempt = self.jobs[index].attempts;
            let delay = self.retry_delay(attempt);
            self.jobs[index].state = State::Waiting {
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
        self.jobs
            .iter()
            .find(|job| job.job.id == id)
            .map(|job| job.state.clone())
            .ok_or(Error::Unknown)
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
            for i in 0..self.jobs.len() {
                if !matches!(self.jobs[i].state, State::Waiting { .. }) {
                    continue;
                }
                if self.jobs[i].dependencies.iter().any(|&dep| {
                    matches!(self.jobs[dep].state, State::Failed | State::Blocked)
                }) {
                    self.jobs[i].state = State::Blocked;
                    changed = true;
                }
            }
            if !changed {
                break;
            }
        }
    }
}