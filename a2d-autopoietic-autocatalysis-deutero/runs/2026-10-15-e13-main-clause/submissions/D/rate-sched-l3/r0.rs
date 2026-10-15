use std::collections::HashMap;

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
    state: Option<State>,
    attempts: u8,
}

pub struct Runner {
    jobs: Vec<JobRecord>,
    id_to_index: HashMap<String, usize>,
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

        let mut id_to_index = HashMap::new();

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
            if id_to_index.contains_key(&job.id) {
                return Err(Error::Duplicate(i));
            }
            id_to_index.insert(job.id.clone(), i);
        }

        let mut resolved_dependencies = Vec::with_capacity(jobs.len());
        for (i, job) in jobs.iter().enumerate() {
            let mut seen = HashMap::<&str, ()>::new();
            let mut deps = Vec::with_capacity(job.dependencies.len());
            for (j, dependency) in job.dependencies.iter().enumerate() {
                if seen.insert(dependency.as_str(), ()).is_some() {
                    return Err(Error::DuplicateDependency(i, j));
                }
                match id_to_index.get(dependency) {
                    Some(&index) => deps.push(index),
                    None => return Err(Error::MissingDependency(i, j)),
                }
            }
            resolved_dependencies.push(deps);
        }

        if has_cycle(&resolved_dependencies) {
            return Err(Error::Cycle);
        }

        let records = jobs
            .into_iter()
            .zip(resolved_dependencies)
            .map(|(job, dependencies)| JobRecord {
                job,
                dependencies,
                state: None,
                attempts: 0,
            })
            .collect();

        Ok(Self {
            jobs: records,
            id_to_index,
            aging_ms,
            retry_base,
            retry_cap,
            last_time: 0,
            cycle_started: false,
        })
    }

    pub fn begin_cycle(&mut self, now: u64) -> Result<(), Error> {
        self.check_time(now)?;
        if !self.cycle_started {
            self.last_time = now;
            self.cycle_started = true;
            for record in &mut self.jobs {
                record.state = Some(State::Waiting { ready_at: now });
                record.attempts = 0;
            }
            return Ok(());
        }

        if self.jobs.iter().any(|record| {
            matches!(
                record.state,
                Some(State::Waiting { .. } | State::Running { .. })
            )
        }) {
            return Err(Error::Busy);
        }

        self.last_time = now;
        for record in &mut self.jobs {
            record.state = Some(State::Waiting { ready_at: now });
            record.attempts = 0;
        }
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
                Some(State::Waiting { ready_at }) if ready_at <= now => ready_at,
                _ => continue,
            };
            if !record.dependencies.iter().all(|&dep| {
                matches!(self.jobs[dep].state, Some(State::Succeeded))
            }) {
                continue;
            }

            let effective = if self.aging_ms == 0 {
                record.job.priority
            } else {
                let age_steps = (now - ready_at) / self.aging_ms;
                (record.job.priority as u64)
                    .saturating_add(age_steps)
                    .min(9) as u8
            };

            match best {
                None => best = Some((i, effective)),
                Some((best_i, best_priority))
                    if effective > best_priority
                        || (effective == best_priority && i < best_i) =>
                {
                    best = Some((i, effective));
                }
                _ => {}
            }
        }

        self.last_time = now;
        let Some((index, _)) = best else {
            return Ok(None);
        };

        let record = &mut self.jobs[index];
        record.attempts = record.attempts.saturating_add(1);
        record.state = Some(State::Running {
            attempt: record.attempts,
        });
        Ok(Some(Ticket {
            id: record.job.id.clone(),
            attempt: record.attempts,
        }))
    }

    pub fn complete(&mut self, now: u64, id: &str, success: bool) -> Result<(), Error> {
        self.check_time(now)?;
        if !self.cycle_started {
            return Err(Error::NoCycle);
        }
        let index = *self.id_to_index.get(id).ok_or(Error::Unknown)?;
        let attempt = match self.jobs[index].state {
            Some(State::Running { attempt }) => attempt,
            _ => return Err(Error::NotRunning),
        };

        self.last_time = now;
        if success {
            self.jobs[index].state = Some(State::Succeeded);
            return Ok(());
        }

        if attempt == self.jobs[index].job.max_attempts {
            self.jobs[index].state = Some(State::Failed);
            self.propagate_blocked();
        } else {
            let delay = retry_delay(self.retry_base, self.retry_cap, attempt);
            self.jobs[index].state = Some(State::Waiting {
                ready_at: now.saturating_add(delay),
            });
        }
        Ok(())
    }

    pub fn status(&self, id: &str) -> Result<State, Error> {
        if !self.cycle_started {
            return Err(Error::NoCycle);
        }
        let index = *self.id_to_index.get(id).ok_or(Error::Unknown)?;
        self.jobs[index].state.clone().ok_or(Error::NoCycle)
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
                if !matches!(self.jobs[i].state, Some(State::Waiting { .. })) {
                    continue;
                }
                let blocked = self.jobs[i].dependencies.iter().any(|&dep| {
                    matches!(
                        self.jobs[dep].state,
                        Some(State::Failed | State::Blocked)
                    )
                });
                if blocked {
                    self.jobs[i].state = Some(State::Blocked);
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
    fn visit(node: usize, deps: &[Vec<usize>], marks: &mut [u8]) -> bool {
        match marks[node] {
            1 => return true,
            2 => return false,
            _ => {}
        }
        marks[node] = 1;
        for &dependency in &deps[node] {
            if visit(dependency, deps, marks) {
                return true;
            }
        }
        marks[node] = 2;
        false
    }

    let mut marks = vec![0; dependencies.len()];
    for node in 0..dependencies.len() {
        if visit(node, dependencies, &mut marks) {
            return true;
        }
    }
    false
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