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
    aging_ms: u64,
    retry_base: u64,
    retry_cap: u64,
    states: Vec<JobState>,
    id_to_index: std::collections::HashMap<String, usize>,
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

        let mut id_to_index = std::collections::HashMap::new();
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
            if job.max_attempts == 0 || job.max_attempts > 32 {
                return Err(Error::BadAttempts(i));
            }
            if id_to_index.contains_key(&job.id) {
                return Err(Error::Duplicate(i));
            }
            id_to_index.insert(job.id.clone(), i);
        }

        let mut dependencies = vec![Vec::new(); jobs.len()];
        for (i, job) in jobs.iter().enumerate() {
            let mut seen = std::collections::HashSet::new();
            for (d, dep) in job.dependencies.iter().enumerate() {
                if !seen.insert(dep.as_str()) {
                    return Err(Error::DuplicateDependency(i, d));
                }
                match id_to_index.get(dep) {
                    Some(&index) => dependencies[i].push(index),
                    None => return Err(Error::MissingDependency(i, d)),
                }
            }
        }

        fn visit(node: usize, deps: &[Vec<usize>], marks: &mut [u8]) -> bool {
            if marks[node] == 1 {
                return true;
            }
            if marks[node] == 2 {
                return false;
            }
            marks[node] = 1;
            for &dep in &deps[node] {
                if visit(dep, deps, marks) {
                    return true;
                }
            }
            marks[node] = 2;
            false
        }

        let mut marks = vec![0; jobs.len()];
        for node in 0..jobs.len() {
            if visit(node, &dependencies, &mut marks) {
                return Err(Error::Cycle);
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
            aging_ms,
            retry_base,
            retry_cap,
            states,
            id_to_index,
            last_time: 0,
            has_cycle: false,
        })
    }

    pub fn begin_cycle(&mut self, now: u64) -> Result<(), Error> {
        if self.has_cycle && now < self.last_time {
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
        if self.has_cycle && now < self.last_time {
            return Err(Error::TimeReversed);
        }
        if !self.has_cycle {
            return Err(Error::NoCycle);
        }

        let mut selected: Option<(usize, u8)> = None;
        for i in 0..self.jobs.len() {
            let ready_at = match self.states[i].state {
                State::Waiting { ready_at } if ready_at <= now => ready_at,
                _ => continue,
            };
            if !self.dependencies[i]
                .iter()
                .all(|&d| matches!(self.states[d].state, State::Succeeded))
            {
                continue;
            }

            let priority = if self.aging_ms == 0 {
                self.jobs[i].priority
            } else {
                let age = (now - ready_at) / self.aging_ms;
                (u64::from(self.jobs[i].priority) + age).min(9) as u8
            };
            if selected.is_none_or(|(best_i, best_priority)| {
                priority > best_priority || (priority == best_priority && i < best_i)
            }) {
                selected = Some((i, priority));
            }
        }

        self.last_time = now;
        let Some((i, _)) = selected else {
            return Ok(None);
        };

        self.states[i].attempts += 1;
        let attempt = self.states[i].attempts;
        self.states[i].state = State::Running { attempt };
        Ok(Some(Ticket {
            id: self.jobs[i].id.clone(),
            attempt,
        }))
    }

    pub fn complete(&mut self, now: u64, id: &str, success: bool) -> Result<(), Error> {
        if self.has_cycle && now < self.last_time {
            return Err(Error::TimeReversed);
        }
        if !self.has_cycle {
            return Err(Error::NoCycle);
        }
        let i = *self.id_to_index.get(id).ok_or(Error::Unknown)?;
        let attempt = match self.states[i].state {
            State::Running { attempt } => attempt,
            _ => return Err(Error::NotRunning),
        };

        if success {
            self.states[i].state = State::Succeeded;
        } else if attempt == self.jobs[i].max_attempts {
            self.states[i].state = State::Failed;
            self.propagate_blocked();
        } else {
            let shift = u32::from(attempt - 1);
            let delay = self
                .retry_base
                .checked_shl(shift)
                .unwrap_or(u64::MAX)
                .min(self.retry_cap);
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
        let i = *self.id_to_index.get(id).ok_or(Error::Unknown)?;
        Ok(self.states[i].state.clone())
    }

    fn propagate_blocked(&mut self) {
        loop {
            let mut changed = false;
            for i in 0..self.jobs.len() {
                if !matches!(self.states[i].state, State::Waiting { .. }) {
                    continue;
                }
                if self.dependencies[i].iter().any(|&d| {
                    matches!(self.states[d].state, State::Failed | State::Blocked)
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