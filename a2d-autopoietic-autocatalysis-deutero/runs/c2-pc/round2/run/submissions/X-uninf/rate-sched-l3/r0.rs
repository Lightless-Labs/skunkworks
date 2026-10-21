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
            let mut deps = Vec::with_capacity(job.dependencies.len());
            for (j, dep) in job.dependencies.iter().enumerate() {
                if !seen.insert(dep.as_str()) {
                    return Err(Error::DuplicateDependency(i, j));
                }
                match ids.get(dep.as_str()) {
                    Some(&index) => deps.push(index),
                    None => return Err(Error::MissingDependency(i, j)),
                }
            }
            dependencies.push(deps);
        }

        // Iterative depth-first traversal avoids using the call stack for graph depth.
        let mut marks = vec![0u8; jobs.len()];
        for start in 0..jobs.len() {
            if marks[start] != 0 {
                continue;
            }

            marks[start] = 1;
            let mut stack = vec![(start, 0usize)];
            while let Some((node, next_dependency)) = stack.last_mut() {
                if *next_dependency == dependencies[*node].len() {
                    marks[*node] = 2;
                    stack.pop();
                    continue;
                }

                let dep = dependencies[*node][*next_dependency];
                *next_dependency += 1;
                match marks[dep] {
                    0 => {
                        marks[dep] = 1;
                        stack.push((dep, 0));
                    }
                    1 => return Err(Error::Cycle),
                    _ => {}
                }
            }
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
            has_cycle: false,
        })
    }

    pub fn begin_cycle(&mut self, now: u64) -> Result<(), Error> {
        if now < self.last_time {
            return Err(Error::TimeReversed);
        }
        if self.has_cycle
            && self.states.iter().any(|s| {
                matches!(s.state, State::Waiting { .. } | State::Running { .. })
            })
        {
            return Err(Error::Busy);
        }

        for state in &mut self.states {
            state.state = State::Waiting { ready_at: now };
            state.attempts = 0;
        }
        self.has_cycle = true;
        self.last_time = now;
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
        for i in 0..self.jobs.len() {
            let ready_at = match self.states[i].state {
                State::Waiting { ready_at } if ready_at <= now => ready_at,
                _ => continue,
            };
            if !self.dependencies[i]
                .iter()
                .all(|&dep| matches!(self.states[dep].state, State::Succeeded))
            {
                continue;
            }

            let base = self.jobs[i].priority;
            let effective = if self.aging_ms == 0 {
                base
            } else {
                let age = (now - ready_at) / self.aging_ms;
                // Since the result is capped at 9, compare the age against
                // the small remaining priority range before adding it.
                if age >= u64::from(9 - base) {
                    9
                } else {
                    base + age as u8
                }
            };
            if best.map_or(true, |(_, p)| effective > p) {
                best = Some((i, effective));
            }
        }

        self.last_time = now;
        if let Some((i, _)) = best {
            self.states[i].attempts += 1;
            let attempt = self.states[i].attempts;
            self.states[i].state = State::Running { attempt };
            Ok(Some(Ticket {
                id: self.jobs[i].id.clone(),
                attempt,
            }))
        } else {
            Ok(None)
        }
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
            .position(|job| job.id == id)
            .ok_or(Error::Unknown)?;
        let attempt = match self.states[index].state {
            State::Running { attempt } => attempt,
            _ => return Err(Error::NotRunning),
        };

        self.last_time = now;
        if success {
            self.states[index].state = State::Succeeded;
            return Ok(());
        }

        if attempt == self.jobs[index].max_attempts {
            self.states[index].state = State::Failed;
            self.propagate_blocked();
        } else {
            let shift = (attempt - 1) as u32;
            let delay = self
                .retry_base
                .checked_mul(1u64.checked_shl(shift).unwrap_or(u64::MAX))
                .unwrap_or(u64::MAX)
                .min(self.retry_cap);
            self.states[index].state = State::Waiting {
                ready_at: now.saturating_add(delay),
            };
        }
        Ok(())
    }

    fn propagate_blocked(&mut self) {
        loop {
            let mut changed = false;
            for i in 0..self.jobs.len() {
                if !matches!(self.states[i].state, State::Waiting { .. }) {
                    continue;
                }
                if self.dependencies[i].iter().any(|&dep| {
                    matches!(self.states[dep].state, State::Failed | State::Blocked)
                }) {
                    self.states[i].state = State::Blocked;
                    changed = true;
                }
            }
            if !changed {
                break;
            }
        }
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
        Ok(self.states[index].state.clone())
    }
}