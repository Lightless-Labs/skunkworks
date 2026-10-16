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

        use std::collections::HashMap;

        let mut indices = HashMap::new();
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
            if indices.contains_key(&job.id) {
                return Err(Error::Duplicate(i));
            }
            indices.insert(job.id.clone(), i);
        }

        let mut resolved = Vec::with_capacity(jobs.len());
        for (i, job) in jobs.iter().enumerate() {
            let mut seen = HashMap::<&str, ()>::new();
            let mut deps = Vec::with_capacity(job.dependencies.len());
            for (j, dep) in job.dependencies.iter().enumerate() {
                if seen.insert(dep.as_str(), ()).is_some() {
                    return Err(Error::DuplicateDependency(i, j));
                }
                match indices.get(dep) {
                    Some(&index) => deps.push(index),
                    None => return Err(Error::MissingDependency(i, j)),
                }
            }
            resolved.push(deps);
        }

        if has_cycle(&resolved) {
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
            cycle_started: false,
        })
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
            let ready_at = match record.state {
                State::Waiting { ready_at } if ready_at <= now => ready_at,
                _ => continue,
            };
            if !record
                .dependencies
                .iter()
                .all(|&dep| matches!(self.jobs[dep].state, State::Succeeded))
            {
                continue;
            }

            let effective = if self.aging_ms == 0 {
                record.job.priority
            } else {
                let age = (now - ready_at) / self.aging_ms;
                (u64::from(record.job.priority) + age).min(9) as u8
            };

            if best.map_or(true, |(_, priority)| effective > priority) {
                best = Some((i, effective));
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

        let index = self
            .jobs
            .iter()
            .position(|record| record.job.id == id)
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
            let delay = self.retry_delay(self.jobs[index].attempts);
            self.jobs[index].state = State::Waiting {
                ready_at: now.saturating_add(delay),
            };
        }

        self.last_time = now;
        Ok(())
    }

    pub fn status(&self, id: &str) -> Result<State, Error> {
        if !self.cycle_started {
            return Err(Error::NoCycle);
        }
        self.jobs
            .iter()
            .find(|record| record.job.id == id)
            .map(|record| record.state.clone())
            .ok_or(Error::Unknown)
    }

    fn check_time(&self, now: u64) -> Result<(), Error> {
        if now < self.last_time {
            Err(Error::TimeReversed)
        } else {
            Ok(())
        }
    }

    fn retry_delay(&self, attempt: u8) -> u64 {
        let mut delay = self.retry_base;
        for _ in 1..attempt {
            if delay >= self.retry_cap {
                return self.retry_cap;
            }
            delay = delay.saturating_mul(2).min(self.retry_cap);
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
                let doomed = self.jobs[i].dependencies.iter().any(|&dep| {
                    matches!(self.jobs[dep].state, State::Failed | State::Blocked)
                });
                if doomed {
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

fn valid_id(id: &str) -> bool {
    (1..=16).contains(&id.len())
        && id
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
}

fn has_cycle(dependencies: &[Vec<usize>]) -> bool {
    fn visit(node: usize, graph: &[Vec<usize>], marks: &mut [u8]) -> bool {
        if marks[node] == 1 {
            return true;
        }
        if marks[node] == 2 {
            return false;
        }
        marks[node] = 1;
        for &next in &graph[node] {
            if visit(next, graph, marks) {
                return true;
            }
        }
        marks[node] = 2;
        false
    }

    let mut marks = vec![0; dependencies.len()];
    (0..dependencies.len()).any(|node| visit(node, dependencies, &mut marks))
}