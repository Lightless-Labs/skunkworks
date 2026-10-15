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

pub struct Runner {
    jobs: Vec<Job>,
    dependencies: Vec<Vec<usize>>,
    states: Vec<State>,
    attempts: Vec<u8>,
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
            if job.id.is_empty()
                || job.id.len() > 16
                || !job
                    .id
                    .bytes()
                    .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
            {
                return Err(Error::BadId(i));
            }
            if job.priority > 9 {
                return Err(Error::BadPriority(i));
            }
            if job.max_attempts == 0 || job.max_attempts > 32 {
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
                    Some(&idx) => deps.push(idx),
                    None => return Err(Error::MissingDependency(i, j)),
                }
            }
            dependencies.push(deps);
        }

        let mut indegree = vec![0usize; jobs.len()];
        let mut dependents = vec![Vec::new(); jobs.len()];
        for (job_idx, deps) in dependencies.iter().enumerate() {
            indegree[job_idx] = deps.len();
            for &dep in deps {
                dependents[dep].push(job_idx);
            }
        }
        let mut queue = std::collections::VecDeque::new();
        for (i, &degree) in indegree.iter().enumerate() {
            if degree == 0 {
                queue.push_back(i);
            }
        }
        let mut visited = 0usize;
        while let Some(idx) = queue.pop_front() {
            visited += 1;
            for &child in &dependents[idx] {
                indegree[child] -= 1;
                if indegree[child] == 0 {
                    queue.push_back(child);
                }
            }
        }
        if visited != jobs.len() {
            return Err(Error::Cycle);
        }

        let count = jobs.len();
        Ok(Self {
            jobs,
            dependencies,
            states: vec![State::Waiting { ready_at: 0 }; count],
            attempts: vec![0; count],
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
            && self.states.iter().any(|state| {
                matches!(state, State::Waiting { .. } | State::Running { .. })
            })
        {
            return Err(Error::Busy);
        }

        self.states = vec![State::Waiting { ready_at: now }; self.jobs.len()];
        self.attempts.fill(0);
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

        let mut best: Option<(usize, u8)> = None;
        for i in 0..self.jobs.len() {
            let ready_at = match self.states[i] {
                State::Waiting { ready_at } if ready_at <= now => ready_at,
                _ => continue,
            };
            if !self.dependencies[i]
                .iter()
                .all(|&dep| matches!(self.states[dep], State::Succeeded))
            {
                continue;
            }

            let priority = if self.aging_ms == 0 {
                self.jobs[i].priority
            } else {
                let age = (now - ready_at) / self.aging_ms;
                let effective = u64::from(self.jobs[i].priority).saturating_add(age).min(9);
                effective as u8
            };

            if best.map_or(true, |(best_i, best_priority)| {
                priority > best_priority || (priority == best_priority && i < best_i)
            }) {
                best = Some((i, priority));
            }
        }

        self.last_time = now;
        let Some((idx, _)) = best else {
            return Ok(None);
        };

        self.attempts[idx] += 1;
        let attempt = self.attempts[idx];
        self.states[idx] = State::Running { attempt };
        Ok(Some(Ticket {
            id: self.jobs[idx].id.clone(),
            attempt,
        }))
    }

    pub fn complete(&mut self, now: u64, id: &str, success: bool) -> Result<(), Error> {
        if now < self.last_time {
            return Err(Error::TimeReversed);
        }
        if !self.cycle_started {
            return Err(Error::NoCycle);
        }
        let idx = self
            .jobs
            .iter()
            .position(|job| job.id == id)
            .ok_or(Error::Unknown)?;
        if !matches!(self.states[idx], State::Running { .. }) {
            return Err(Error::NotRunning);
        }

        self.last_time = now;
        if success {
            self.states[idx] = State::Succeeded;
            return Ok(());
        }

        let attempt = self.attempts[idx];
        if attempt == self.jobs[idx].max_attempts {
            self.states[idx] = State::Failed;
            self.propagate_blocked();
        } else {
            let shift = u32::from(attempt.saturating_sub(1));
            let delay = self
                .retry_base
                .checked_shl(shift)
                .unwrap_or(u64::MAX)
                .min(self.retry_cap);
            self.states[idx] = State::Waiting {
                ready_at: now.saturating_add(delay),
            };
        }
        Ok(())
    }

    pub fn status(&self, id: &str) -> Result<State, Error> {
        if !self.cycle_started {
            return Err(Error::NoCycle);
        }
        let idx = self
            .jobs
            .iter()
            .position(|job| job.id == id)
            .ok_or(Error::Unknown)?;
        Ok(self.states[idx].clone())
    }

    fn propagate_blocked(&mut self) {
        loop {
            let mut changed = false;
            for i in 0..self.jobs.len() {
                if matches!(self.states[i], State::Waiting { .. })
                    && self.dependencies[i]
                        .iter()
                        .any(|&dep| matches!(self.states[dep], State::Failed | State::Blocked))
                {
                    self.states[i] = State::Blocked;
                    changed = true;
                }
            }
            if !changed {
                break;
            }
        }
    }
}