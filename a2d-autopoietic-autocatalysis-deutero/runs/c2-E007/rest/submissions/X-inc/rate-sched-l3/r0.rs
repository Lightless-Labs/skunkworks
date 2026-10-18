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

        // Iterative depth-first search avoids stack overflow on long graphs.
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

                let dependency = dependencies[*node][*next_dependency];
                *next_dependency += 1;
                match marks[dependency] {
                    1 => return Err(Error::Cycle),
                    2 => {}
                    _ => {
                        marks[dependency] = 1;
                        stack.push((dependency, 0));
                    }
                }
            }
        }

        let records = jobs
            .into_iter()
            .enumerate()
            .map(|(i, job)| JobRecord {
                job,
                dependencies: dependencies[i].clone(),
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

    fn index_of(&self, id: &str) -> Option<usize> {
        self.jobs.iter().position(|record| record.job.id == id)
    }

    pub fn begin_cycle(&mut self, now: u64) -> Result<(), Error> {
        self.check_time(now)?;
        if self.cycle_started
            && self.jobs.iter().any(|record| {
                matches!(record.state, State::Waiting { .. } | State::Running { .. })
            })
        {
            return Err(Error::Busy);
        }

        for record in &mut self.jobs {
            record.state = State::Waiting { ready_at: now };
            record.attempts = 0;
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
        for (i, record) in self.jobs.iter().enumerate() {
            let State::Waiting { ready_at } = record.state else {
                continue;
            };
            if ready_at > now
                || !record
                    .dependencies
                    .iter()
                    .all(|&dependency| matches!(self.jobs[dependency].state, State::Succeeded))
            {
                continue;
            }

            let effective_priority = if self.aging_ms == 0 {
                record.job.priority
            } else {
                let age = (now - ready_at) / self.aging_ms;
                (u64::from(record.job.priority) + age).min(9) as u8
            };

            // Keeping the earlier index on ties preserves input order.
            if best.map_or(true, |(_, priority)| effective_priority > priority) {
                best = Some((i, effective_priority));
            }
        }

        let Some((index, _)) = best else {
            self.last_time = now;
            return Ok(None);
        };

        let record = &mut self.jobs[index];
        record.attempts += 1;
        record.state = State::Running {
            attempt: record.attempts,
        };
        let ticket = Ticket {
            id: record.job.id.clone(),
            attempt: record.attempts,
        };
        self.last_time = now;
        Ok(Some(ticket))
    }

    pub fn complete(&mut self, now: u64, id: &str, success: bool) -> Result<(), Error> {
        self.check_time(now)?;
        if !self.cycle_started {
            return Err(Error::NoCycle);
        }
        let index = self.index_of(id).ok_or(Error::Unknown)?;
        if !matches!(self.jobs[index].state, State::Running { .. }) {
            return Err(Error::NotRunning);
        }

        if success {
            self.jobs[index].state = State::Succeeded;
        } else if self.jobs[index].attempts == self.jobs[index].job.max_attempts {
            self.jobs[index].state = State::Failed;
        } else {
            let attempt = self.jobs[index].attempts;
            let mut delay = self.retry_base;
            for _ in 1..attempt {
                delay = delay.saturating_mul(2).min(self.retry_cap);
            }
            self.jobs[index].state = State::Waiting {
                ready_at: now.saturating_add(delay),
            };
        }

        if matches!(self.jobs[index].state, State::Failed) {
            loop {
                let mut changed = false;
                for i in 0..self.jobs.len() {
                    if !matches!(self.jobs[i].state, State::Waiting { .. }) {
                        continue;
                    }
                    if self.jobs[i].dependencies.iter().any(|&dependency| {
                        matches!(
                            self.jobs[dependency].state,
                            State::Failed | State::Blocked
                        )
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

        self.last_time = now;
        Ok(())
    }

    pub fn status(&self, id: &str) -> Result<State, Error> {
        if !self.cycle_started {
            return Err(Error::NoCycle);
        }
        let index = self.index_of(id).ok_or(Error::Unknown)?;
        Ok(self.jobs[index].state.clone())
    }
}