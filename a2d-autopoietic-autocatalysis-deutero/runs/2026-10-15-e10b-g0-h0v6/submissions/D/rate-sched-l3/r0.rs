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
    attempts: u8,
    state: Option<State>,
}

pub struct Runner {
    jobs: Vec<JobRecord>,
    aging_ms: u64,
    retry_base: u64,
    retry_cap: u64,
    last_time: u64,
    has_time: bool,
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

        let mut seen = std::collections::HashMap::new();
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
            if seen.insert(job.id.as_str(), index).is_some() {
                return Err(Error::Duplicate(index));
            }
        }

        let mut resolved = Vec::with_capacity(jobs.len());
        for (job_index, job) in jobs.iter().enumerate() {
            let mut dependency_names = std::collections::HashSet::new();
            let mut dependencies = Vec::with_capacity(job.dependencies.len());
            for (dependency_index, dependency) in job.dependencies.iter().enumerate() {
                if !dependency_names.insert(dependency.as_str()) {
                    return Err(Error::DuplicateDependency(job_index, dependency_index));
                }
                match seen.get(dependency.as_str()) {
                    Some(&index) => dependencies.push(index),
                    None => return Err(Error::MissingDependency(job_index, dependency_index)),
                }
            }
            resolved.push(dependencies);
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
                attempts: 0,
                state: None,
            })
            .collect();

        Ok(Self {
            jobs: records,
            aging_ms,
            retry_base,
            retry_cap,
            last_time: 0,
            has_time: false,
            cycle_started: false,
        })
    }

    pub fn begin_cycle(&mut self, now: u64) -> Result<(), Error> {
        self.check_time(now)?;
        if self.cycle_started
            && self.jobs.iter().any(|record| {
                matches!(
                    record.state,
                    Some(State::Waiting { .. }) | Some(State::Running { .. })
                )
            })
        {
            return Err(Error::Busy);
        }

        for record in &mut self.jobs {
            record.attempts = 0;
            record.state = Some(State::Waiting { ready_at: now });
        }
        self.cycle_started = true;
        self.record_time(now);
        Ok(())
    }

    pub fn next_ready(&mut self, now: u64) -> Result<Option<Ticket>, Error> {
        self.check_time(now)?;
        if !self.cycle_started {
            return Err(Error::NoCycle);
        }

        let mut best: Option<(usize, u8)> = None;
        for (index, record) in self.jobs.iter().enumerate() {
            let ready_at = match record.state {
                Some(State::Waiting { ready_at }) if ready_at <= now => ready_at,
                _ => continue,
            };
            if !record.dependencies.iter().all(|&dependency| {
                matches!(self.jobs[dependency].state, Some(State::Succeeded))
            }) {
                continue;
            }

            let priority = effective_priority(
                record.job.priority,
                now.saturating_sub(ready_at),
                self.aging_ms,
            );
            if best.map_or(true, |(best_index, best_priority)| {
                priority > best_priority || (priority == best_priority && index < best_index)
            }) {
                best = Some((index, priority));
            }
        }

        let ticket = if let Some((index, _)) = best {
            let record = &mut self.jobs[index];
            record.attempts += 1;
            let attempt = record.attempts;
            record.state = Some(State::Running { attempt });
            Some(Ticket {
                id: record.job.id.clone(),
                attempt,
            })
        } else {
            None
        };

        self.record_time(now);
        Ok(ticket)
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

        if !matches!(self.jobs[index].state, Some(State::Running { .. })) {
            return Err(Error::NotRunning);
        }

        if success {
            self.jobs[index].state = Some(State::Succeeded);
        } else if self.jobs[index].attempts == self.jobs[index].job.max_attempts {
            self.jobs[index].state = Some(State::Failed);
            self.propagate_blocked();
        } else {
            let attempt = self.jobs[index].attempts;
            let delay = retry_delay(self.retry_base, self.retry_cap, attempt);
            self.jobs[index].state = Some(State::Waiting {
                ready_at: now.saturating_add(delay),
            });
        }

        self.record_time(now);
        Ok(())
    }

    pub fn status(&self, id: &str) -> Result<State, Error> {
        if !self.cycle_started {
            return Err(Error::NoCycle);
        }
        let record = self
            .jobs
            .iter()
            .find(|record| record.job.id == id)
            .ok_or(Error::Unknown)?;
        Ok(record.state.as_ref().expect("cycle states are initialized").clone())
    }

    fn check_time(&self, now: u64) -> Result<(), Error> {
        if self.has_time && now < self.last_time {
            Err(Error::TimeReversed)
        } else {
            Ok(())
        }
    }

    fn record_time(&mut self, now: u64) {
        self.last_time = now;
        self.has_time = true;
    }

    fn propagate_blocked(&mut self) {
        loop {
            let newly_blocked: Vec<usize> = self
                .jobs
                .iter()
                .enumerate()
                .filter_map(|(index, record)| {
                    if !matches!(record.state, Some(State::Waiting { .. })) {
                        return None;
                    }
                    record
                        .dependencies
                        .iter()
                        .any(|&dependency| {
                            matches!(
                                self.jobs[dependency].state,
                                Some(State::Failed | State::Blocked)
                            )
                        })
                        .then_some(index)
                })
                .collect();

            if newly_blocked.is_empty() {
                break;
            }
            for index in newly_blocked {
                self.jobs[index].state = Some(State::Blocked);
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

fn effective_priority(base: u8, elapsed: u64, aging_ms: u64) -> u8 {
    if aging_ms == 0 {
        return base;
    }
    let increment = elapsed / aging_ms;
    (u64::from(base) + increment).min(9) as u8
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

fn has_cycle(dependencies: &[Vec<usize>]) -> bool {
    fn visit(node: usize, dependencies: &[Vec<usize>], marks: &mut [u8]) -> bool {
        match marks[node] {
            1 => return true,
            2 => return false,
            _ => {}
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

    let mut marks = vec![0; dependencies.len()];
    (0..dependencies.len()).any(|node| visit(node, dependencies, &mut marks))
}