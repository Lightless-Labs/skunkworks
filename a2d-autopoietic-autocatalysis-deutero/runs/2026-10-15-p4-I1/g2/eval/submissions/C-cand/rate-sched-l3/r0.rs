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
    state: Option<State>,
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

        fn has_cycle(dependencies: &[Vec<usize>]) -> bool {
            fn visit(node: usize, deps: &[Vec<usize>], marks: &mut [u8]) -> bool {
                if marks[node] == 1 {
                    return true;
                }
                if marks[node] == 2 {
                    return false;
                }
                marks[node] = 1;
                for &next in &deps[node] {
                    if visit(next, deps, marks) {
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
                state: None,
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
            && self.states.iter().any(|entry| {
                matches!(
                    entry.state,
                    Some(State::Waiting { .. } | State::Running { .. })
                )
            })
        {
            return Err(Error::Busy);
        }

        for entry in &mut self.states {
            entry.state = Some(State::Waiting { ready_at: now });
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
        for i in 0..self.jobs.len() {
            let ready_at = match self.states[i].state {
                Some(State::Waiting { ready_at }) if ready_at <= now => ready_at,
                _ => continue,
            };
            if !self.dependencies[i].iter().all(|&dep| {
                matches!(self.states[dep].state, Some(State::Succeeded))
            }) {
                continue;
            }

            let base = self.jobs[i].priority;
            let effective = if self.aging_ms == 0 {
                base
            } else {
                let aging = (now - ready_at) / self.aging_ms;
                (u64::from(base) + aging).min(9) as u8
            };

            if best.map_or(true, |(best_i, best_priority)| {
                effective > best_priority || (effective == best_priority && i < best_i)
            }) {
                best = Some((i, effective));
            }
        }

        let result = if let Some((i, _)) = best {
            self.states[i].attempts += 1;
            let attempt = self.states[i].attempts;
            self.states[i].state = Some(State::Running { attempt });
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
        if now < self.last_time {
            return Err(Error::TimeReversed);
        }
        if !self.cycle_started {
            return Err(Error::NoCycle);
        }

        let index = self
            .jobs
            .iter()
            .position(|job| job.id == id)
            .ok_or(Error::Unknown)?;

        if !matches!(self.states[index].state, Some(State::Running { .. })) {
            return Err(Error::NotRunning);
        }

        if success {
            self.states[index].state = Some(State::Succeeded);
        } else if self.states[index].attempts == self.jobs[index].max_attempts {
            self.states[index].state = Some(State::Failed);
            self.propagate_blocked();
        } else {
            let attempt = self.states[index].attempts;
            let mut delay = self.retry_base;
            for _ in 1..attempt {
                delay = delay.saturating_mul(2).min(self.retry_cap);
            }
            delay = delay.min(self.retry_cap);
            self.states[index].state = Some(State::Waiting {
                ready_at: now.saturating_add(delay),
            });
        }

        self.last_time = now;
        Ok(())
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
        self.states[index].state.clone().ok_or(Error::NoCycle)
    }

    fn propagate_blocked(&mut self) {
        loop {
            let mut changed = false;
            for i in 0..self.jobs.len() {
                if !matches!(self.states[i].state, Some(State::Waiting { .. })) {
                    continue;
                }
                if self.dependencies[i].iter().any(|&dep| {
                    matches!(
                        self.states[dep].state,
                        Some(State::Failed | State::Blocked)
                    )
                }) {
                    self.states[i].state = Some(State::Blocked);
                    changed = true;
                }
            }
            if !changed {
                break;
            }
        }
    }
}

fn main() {}