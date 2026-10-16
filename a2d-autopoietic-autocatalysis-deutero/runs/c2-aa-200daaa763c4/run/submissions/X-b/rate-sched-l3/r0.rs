use std::collections::{HashMap, HashSet};

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
    id: String,
    priority: u8,
    dependencies: Vec<usize>,
    max_attempts: u8,
    attempts: u8,
    state: State,
}

pub struct Runner {
    jobs: Vec<JobData>,
    id_to_index: HashMap<String, usize>,
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

        let mut id_to_index = HashMap::new();
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
            if id_to_index.contains_key(&job.id) {
                return Err(Error::Duplicate(i));
            }
            id_to_index.insert(job.id.clone(), i);
        }

        let mut resolved_dependencies = Vec::with_capacity(jobs.len());
        for (i, job) in jobs.iter().enumerate() {
            let mut seen = HashSet::new();
            let mut dependencies = Vec::with_capacity(job.dependencies.len());
            for (j, dependency) in job.dependencies.iter().enumerate() {
                if !seen.insert(dependency.as_str()) {
                    return Err(Error::DuplicateDependency(i, j));
                }
                match id_to_index.get(dependency) {
                    Some(&index) => dependencies.push(index),
                    None => return Err(Error::MissingDependency(i, j)),
                }
            }
            resolved_dependencies.push(dependencies);
        }

        fn visit(node: usize, dependencies: &[Vec<usize>], marks: &mut [u8]) -> bool {
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

        let mut marks = vec![0; jobs.len()];
        for i in 0..jobs.len() {
            if visit(i, &resolved_dependencies, &mut marks) {
                return Err(Error::Cycle);
            }
        }

        let job_data = jobs
            .into_iter()
            .enumerate()
            .map(|(i, job)| JobData {
                id: job.id,
                priority: job.priority,
                dependencies: std::mem::take(&mut resolved_dependencies[i]),
                max_attempts: job.max_attempts,
                attempts: 0,
                state: State::Waiting { ready_at: 0 },
            })
            .collect();

        Ok(Self {
            jobs: job_data,
            id_to_index,
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
            job.attempts = 0;
            job.state = State::Waiting { ready_at: now };
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
            let ready_at = match job.state {
                State::Waiting { ready_at } if ready_at <= now => ready_at,
                _ => continue,
            };
            if !job.dependencies.iter().all(|&dependency| {
                matches!(self.jobs[dependency].state, State::Succeeded)
            }) {
                continue;
            }

            let effective_priority = if self.aging_ms == 0 {
                job.priority
            } else {
                let age = (now - ready_at) / self.aging_ms;
                (u64::from(job.priority) + age).min(9) as u8
            };

            match best {
                None => best = Some((i, effective_priority)),
                Some((_, current)) if effective_priority > current => {
                    best = Some((i, effective_priority));
                }
                _ => {}
            }
        }

        self.last_time = now;
        if let Some((index, _)) = best {
            let job = &mut self.jobs[index];
            job.attempts += 1;
            job.state = State::Running {
                attempt: job.attempts,
            };
            Ok(Some(Ticket {
                id: job.id.clone(),
                attempt: job.attempts,
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
        let index = match self.id_to_index.get(id) {
            Some(&index) => index,
            None => return Err(Error::Unknown),
        };
        if !matches!(self.jobs[index].state, State::Running { .. }) {
            return Err(Error::NotRunning);
        }

        if success {
            self.jobs[index].state = State::Succeeded;
        } else if self.jobs[index].attempts == self.jobs[index].max_attempts {
            self.jobs[index].state = State::Failed;
        } else {
            let attempt = self.jobs[index].attempts;
            let shift = u32::from(attempt - 1);
            let delay = if shift >= 64 {
                self.retry_cap
            } else {
                self.retry_base
                    .saturating_mul(1u64 << shift)
                    .min(self.retry_cap)
            };
            self.jobs[index].state = State::Waiting {
                ready_at: now.saturating_add(delay),
            };
        }

        self.propagate_blocked();
        self.last_time = now;
        Ok(())
    }

    pub fn status(&self, id: &str) -> Result<State, Error> {
        if !self.has_cycle {
            return Err(Error::NoCycle);
        }
        match self.id_to_index.get(id) {
            Some(&index) => Ok(self.jobs[index].state.clone()),
            None => Err(Error::Unknown),
        }
    }

    fn propagate_blocked(&mut self) {
        loop {
            let newly_blocked: Vec<usize> = self
                .jobs
                .iter()
                .enumerate()
                .filter_map(|(i, job)| {
                    if !matches!(job.state, State::Waiting { .. }) {
                        return None;
                    }
                    job.dependencies
                        .iter()
                        .any(|&dependency| {
                            matches!(
                                self.jobs[dependency].state,
                                State::Failed | State::Blocked
                            )
                        })
                        .then_some(i)
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
}