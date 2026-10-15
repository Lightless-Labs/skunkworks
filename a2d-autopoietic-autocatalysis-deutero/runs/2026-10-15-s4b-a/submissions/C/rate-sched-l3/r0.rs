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

        let mut edges = vec![Vec::new(); jobs.len()];
        for (i, job) in jobs.iter().enumerate() {
            let mut seen = std::collections::HashSet::new();
            for (j, dep) in job.dependencies.iter().enumerate() {
                if !seen.insert(dep.as_str()) {
                    return Err(Error::DuplicateDependency(i, j));
                }
                let Some(&dependency_index) = ids.get(dep.as_str()) else {
                    return Err(Error::MissingDependency(i, j));
                };
                edges[i].push(dependency_index);
            }
        }

        fn visit(
            node: usize,
            edges: &[Vec<usize>],
            marks: &mut [u8],
        ) -> bool {
            if marks[node] == 1 {
                return false;
            }
            if marks[node] == 2 {
                return true;
            }
            marks[node] = 1;
            for &next in &edges[node] {
                if !visit(next, edges, marks) {
                    return false;
                }
            }
            marks[node] = 2;
            true
        }

        let mut marks = vec![0; jobs.len()];
        for i in 0..jobs.len() {
            if !visit(i, &edges, &mut marks) {
                return Err(Error::Cycle);
            }
        }

        let states = jobs
            .iter()
            .map(|_| JobState {
                state: State::Waiting { ready_at: 0 },
                attempts: 0,
            })
            .collect();

        Ok(Self {
            jobs,
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
        for (i, job) in self.jobs.iter().enumerate() {
            let State::Waiting { ready_at } = self.states[i].state else {
                continue;
            };
            if ready_at > now {
                continue;
            }
            let dependencies_succeeded = job.dependencies.iter().all(|dependency| {
                let index = self.jobs.iter().position(|candidate| candidate.id == *dependency);
                index.is_some_and(|index| matches!(self.states[index].state, State::Succeeded))
            });
            if !dependencies_succeeded {
                continue;
            }

            let priority = if self.aging_ms == 0 {
                job.priority
            } else {
                let age = (now - ready_at) / self.aging_ms;
                std::cmp::min(9, job.priority as u64 + age) as u8
            };
            if best.is_none_or(|(_, best_priority)| priority > best_priority) {
                best = Some((i, priority));
            }
        }

        self.last_time = now;
        let Some((index, _)) = best else {
            return Ok(None);
        };

        self.states[index].attempts += 1;
        let attempt = self.states[index].attempts;
        self.states[index].state = State::Running { attempt };
        Ok(Some(Ticket {
            id: self.jobs[index].id.clone(),
            attempt,
        }))
    }

    pub fn complete(&mut self, now: u64, id: &str, success: bool) -> Result<(), Error> {
        if now < self.last_time {
            return Err(Error::TimeReversed);
        }
        if !self.has_cycle {
            return Err(Error::NoCycle);
        }
        let Some(index) = self.jobs.iter().position(|job| job.id == id) else {
            return Err(Error::Unknown);
        };
        if !matches!(self.states[index].state, State::Running { .. }) {
            return Err(Error::NotRunning);
        }

        self.last_time = now;
        if success {
            self.states[index].state = State::Succeeded;
            return Ok(());
        }

        let attempt = self.states[index].attempts;
        if attempt == self.jobs[index].max_attempts {
            self.states[index].state = State::Failed;
            self.propagate_blocked();
        } else {
            let shift = u32::from(attempt - 1);
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

    pub fn status(&self, id: &str) -> Result<State, Error> {
        if !self.has_cycle {
            return Err(Error::NoCycle);
        }
        let Some(index) = self.jobs.iter().position(|job| job.id == id) else {
            return Err(Error::Unknown);
        };
        Ok(self.states[index].state.clone())
    }

    fn propagate_blocked(&mut self) {
        loop {
            let mut changed = false;
            for i in 0..self.jobs.len() {
                if !matches!(self.states[i].state, State::Waiting { .. }) {
                    continue;
                }
                let has_terminal_dependency = self.jobs[i].dependencies.iter().any(|dependency| {
                    let index = self.jobs.iter().position(|job| job.id == *dependency);
                    index.is_some_and(|index| {
                        matches!(
                            self.states[index].state,
                            State::Failed | State::Blocked
                        )
                    })
                });
                if has_terminal_dependency {
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

fn main() {}