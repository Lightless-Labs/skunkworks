use std::collections::{HashMap, HashSet, VecDeque};

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
    dependency_indices: Vec<Vec<usize>>,
    id_to_index: HashMap<String, usize>,
    states: Vec<JobState>,
    aging_ms: u64,
    retry_base: u64,
    retry_cap: u64,
    watermark: u64,
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

        let mut id_to_index = HashMap::new();

        for (index, job) in jobs.iter().enumerate() {
            if !valid_id(&job.id) {
                return Err(Error::BadId(index));
            }
            if job.priority > 9 {
                return Err(Error::BadPriority(index));
            }
            if !(1..=32).contains(&job.max_attempts) {
                return Err(Error::BadAttempts(index));
            }
            if id_to_index.contains_key(&job.id) {
                return Err(Error::Duplicate(index));
            }
            id_to_index.insert(job.id.clone(), index);
        }

        let mut dependency_indices = Vec::with_capacity(jobs.len());

        for (job_index, job) in jobs.iter().enumerate() {
            let mut seen = HashSet::new();
            let mut dependencies = Vec::with_capacity(job.dependencies.len());

            for (dependency_index, dependency) in job.dependencies.iter().enumerate() {
                if !seen.insert(dependency.as_str()) {
                    return Err(Error::DuplicateDependency(job_index, dependency_index));
                }

                let Some(&dependency_job_index) = id_to_index.get(dependency) else {
                    return Err(Error::MissingDependency(job_index, dependency_index));
                };
                dependencies.push(dependency_job_index);
            }

            dependency_indices.push(dependencies);
        }

        if has_cycle(&dependency_indices) {
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
            dependency_indices,
            id_to_index,
            states,
            aging_ms,
            retry_base,
            retry_cap,
            watermark: 0,
            cycle_started: false,
        })
    }

    pub fn begin_cycle(&mut self, now: u64) -> Result<(), Error> {
        self.check_time(now)?;

        if self.cycle_started
            && self.states.iter().any(|job_state| {
                matches!(job_state.state, State::Waiting { .. } | State::Running { .. })
            })
        {
            return Err(Error::Busy);
        }

        for job_state in &mut self.states {
            job_state.state = State::Waiting { ready_at: now };
            job_state.attempts = 0;
        }

        self.cycle_started = true;
        self.watermark = now;
        Ok(())
    }

    pub fn next_ready(&mut self, now: u64) -> Result<Option<Ticket>, Error> {
        self.check_time(now)?;
        if !self.cycle_started {
            return Err(Error::NoCycle);
        }

        let mut selected: Option<(usize, u8)> = None;

        for index in 0..self.jobs.len() {
            let State::Waiting { ready_at } = self.states[index].state else {
                continue;
            };
            if ready_at > now {
                continue;
            }
            if !self.dependency_indices[index]
                .iter()
                .all(|&dependency| matches!(self.states[dependency].state, State::Succeeded))
            {
                continue;
            }

            let effective_priority = if self.aging_ms == 0 {
                self.jobs[index].priority
            } else {
                let age = (now - ready_at) / self.aging_ms;
                let aged_priority = u64::from(self.jobs[index].priority).saturating_add(age);
                aged_priority.min(9) as u8
            };

            match selected {
                None => selected = Some((index, effective_priority)),
                Some((_, best_priority)) if effective_priority > best_priority => {
                    selected = Some((index, effective_priority));
                }
                _ => {}
            }
        }

        let result = if let Some((index, _)) = selected {
            let attempt = self.states[index].attempts + 1;
            self.states[index].attempts = attempt;
            self.states[index].state = State::Running { attempt };
            Some(Ticket {
                id: self.jobs[index].id.clone(),
                attempt,
            })
        } else {
            None
        };

        self.watermark = now;
        Ok(result)
    }

    pub fn complete(&mut self, now: u64, id: &str, success: bool) -> Result<(), Error> {
        self.check_time(now)?;
        if !self.cycle_started {
            return Err(Error::NoCycle);
        }

        let Some(&index) = self.id_to_index.get(id) else {
            return Err(Error::Unknown);
        };

        let State::Running { attempt } = self.states[index].state else {
            return Err(Error::NotRunning);
        };

        if success {
            self.states[index].state = State::Succeeded;
        } else if attempt == self.jobs[index].max_attempts {
            self.states[index].state = State::Failed;
            self.propagate_blocked();
        } else {
            let delay = retry_delay(self.retry_base, self.retry_cap, attempt);
            self.states[index].state = State::Waiting {
                ready_at: now.saturating_add(delay),
            };
        }

        self.watermark = now;
        Ok(())
    }

    pub fn status(&self, id: &str) -> Result<State, Error> {
        if !self.cycle_started {
            return Err(Error::NoCycle);
        }
        let Some(&index) = self.id_to_index.get(id) else {
            return Err(Error::Unknown);
        };
        Ok(self.states[index].state.clone())
    }

    fn check_time(&self, now: u64) -> Result<(), Error> {
        if now < self.watermark {
            Err(Error::TimeReversed)
        } else {
            Ok(())
        }
    }

    fn propagate_blocked(&mut self) {
        loop {
            let mut newly_blocked = Vec::new();

            for index in 0..self.jobs.len() {
                if !matches!(self.states[index].state, State::Waiting { .. }) {
                    continue;
                }
                if self.dependency_indices[index].iter().any(|&dependency| {
                    matches!(
                        self.states[dependency].state,
                        State::Failed | State::Blocked
                    )
                }) {
                    newly_blocked.push(index);
                }
            }

            if newly_blocked.is_empty() {
                break;
            }

            for index in newly_blocked {
                self.states[index].state = State::Blocked;
            }
        }
    }
}

fn valid_id(id: &str) -> bool {
    let bytes = id.as_bytes();
    (1..=16).contains(&bytes.len())
        && bytes
            .iter()
            .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || *byte == b'-')
}

fn has_cycle(dependencies: &[Vec<usize>]) -> bool {
    let count = dependencies.len();
    let mut indegree = vec![0usize; count];
    let mut dependents = vec![Vec::new(); count];

    for (job, deps) in dependencies.iter().enumerate() {
        indegree[job] = deps.len();
        for &dependency in deps {
            dependents[dependency].push(job);
        }
    }

    let mut ready = VecDeque::new();
    for (index, &degree) in indegree.iter().enumerate() {
        if degree == 0 {
            ready.push_back(index);
        }
    }

    let mut visited = 0usize;
    while let Some(job) = ready.pop_front() {
        visited += 1;
        for &dependent in &dependents[job] {
            indegree[dependent] -= 1;
            if indegree[dependent] == 0 {
                ready.push_back(dependent);
            }
        }
    }

    visited != count
}

fn retry_delay(retry_base: u64, retry_cap: u64, attempt: u8) -> u64 {
    let mut delay = retry_base;
    for _ in 1..attempt {
        if delay >= retry_cap || delay > retry_cap / 2 {
            return retry_cap;
        }
        delay *= 2;
    }
    delay.min(retry_cap)
}