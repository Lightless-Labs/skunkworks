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

struct JobDef {
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
    jobs: Vec<JobDef>,
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

        if has_cycle(&dependencies) {
            return Err(Error::Cycle);
        }

        let defs: Vec<JobDef> = jobs
            .into_iter()
            .zip(dependencies)
            .map(|(job, dependencies)| JobDef {
                id: job.id,
                priority: job.priority,
                dependencies,
                max_attempts: job.max_attempts,
            })
            .collect();

        let states = defs
            .iter()
            .map(|_| JobState {
                state: State::Waiting { ready_at: 0 },
                attempts: 0,
            })
            .collect();

        Ok(Self {
            jobs: defs,
            states,
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
            && self.states.iter().any(|entry| {
                matches!(entry.state, State::Waiting { .. } | State::Running { .. })
            })
        {
            return Err(Error::Busy);
        }

        for entry in &mut self.states {
            entry.state = State::Waiting { ready_at: now };
            entry.attempts = 0;
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
        for (i, def) in self.jobs.iter().enumerate() {
            let ready_at = match self.states[i].state {
                State::Waiting { ready_at } if ready_at <= now => ready_at,
                _ => continue,
            };
            if !def
                .dependencies
                .iter()
                .all(|&dep| matches!(self.states[dep].state, State::Succeeded))
            {
                continue;
            }

            let priority = if self.aging_ms == 0 {
                def.priority
            } else {
                let age_steps = (now - ready_at) / self.aging_ms;
                (u64::from(def.priority) + age_steps).min(9) as u8
            };

            if best.map_or(true, |(best_i, best_priority)| {
                priority > best_priority || (priority == best_priority && i < best_i)
            }) {
                best = Some((i, priority));
            }
        }

        let result = if let Some((i, _)) = best {
            self.states[i].attempts += 1;
            let attempt = self.states[i].attempts;
            self.states[i].state = State::Running { attempt };
            Some(Ticket {
                id: self.jobs[i].id.clone(),
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
        if !self.has_cycle {
            return Err(Error::NoCycle);
        }

        let i = self
            .jobs
            .iter()
            .position(|job| job.id == id)
            .ok_or(Error::Unknown)?;

        if !matches!(self.states[i].state, State::Running { .. }) {
            return Err(Error::NotRunning);
        }

        if success {
            self.states[i].state = State::Succeeded;
        } else if self.states[i].attempts == self.jobs[i].max_attempts {
            self.states[i].state = State::Failed;
            self.propagate_blocked();
        } else {
            let delay = retry_delay(self.retry_base, self.retry_cap, self.states[i].attempts);
            self.states[i].state = State::Waiting {
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
        let i = self
            .jobs
            .iter()
            .position(|job| job.id == id)
            .ok_or(Error::Unknown)?;
        Ok(self.states[i].state.clone())
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
            let mut changed = false;
            for i in 0..self.jobs.len() {
                if !matches!(self.states[i].state, State::Waiting { .. }) {
                    continue;
                }
                if self.jobs[i].dependencies.iter().any(|&dep| {
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
}

fn valid_id(id: &str) -> bool {
    let bytes = id.as_bytes();
    (1..=16).contains(&bytes.len())
        && bytes
            .iter()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || *b == b'-')
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
        for &dep in &dependencies[node] {
            if visit(dep, dependencies, colors) {
                return true;
            }
        }
        colors[node] = 2;
        false
    }

    let mut colors = vec![0; dependencies.len()];
    (0..dependencies.len()).any(|node| visit(node, dependencies, &mut colors))
}

fn retry_delay(base: u64, cap: u64, attempt: u8) -> u64 {
    let mut delay = base;
    for _ in 1..attempt {
        if delay >= cap || delay > cap / 2 {
            return cap;
        }
        delay *= 2;
    }
    delay.min(cap)
}