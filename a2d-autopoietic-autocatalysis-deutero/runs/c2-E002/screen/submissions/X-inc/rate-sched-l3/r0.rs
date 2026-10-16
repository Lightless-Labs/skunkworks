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
    job: Job,
    dependencies: Vec<usize>,
    state: Option<State>,
    attempts: u8,
}

pub struct Runner {
    jobs: Vec<JobData>,
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

        let mut ids = std::collections::HashMap::<String, usize>::new();
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
            if !(1..=32).contains(&job.max_attempts) {
                return Err(Error::BadAttempts(i));
            }
            if ids.contains_key(&job.id) {
                return Err(Error::Duplicate(i));
            }
            ids.insert(job.id.clone(), i);
        }

        let mut resolved = Vec::with_capacity(jobs.len());
        for (i, job) in jobs.iter().enumerate() {
            let mut seen = std::collections::HashSet::<&str>::new();
            let mut deps = Vec::with_capacity(job.dependencies.len());
            for (j, dep) in job.dependencies.iter().enumerate() {
                if !seen.insert(dep.as_str()) {
                    return Err(Error::DuplicateDependency(i, j));
                }
                match ids.get(dep) {
                    Some(&index) => deps.push(index),
                    None => return Err(Error::MissingDependency(i, j)),
                }
            }
            resolved.push(deps);
        }

        let mut marks = vec![0u8; jobs.len()];
        fn has_cycle(node: usize, deps: &[Vec<usize>], marks: &mut [u8]) -> bool {
            if marks[node] == 1 {
                return true;
            }
            if marks[node] == 2 {
                return false;
            }
            marks[node] = 1;
            for &dep in &deps[node] {
                if has_cycle(dep, deps, marks) {
                    return true;
                }
            }
            marks[node] = 2;
            false
        }
        for i in 0..jobs.len() {
            if has_cycle(i, &resolved, &mut marks) {
                return Err(Error::Cycle);
            }
        }

        let data = jobs
            .into_iter()
            .enumerate()
            .map(|(i, job)| JobData {
                job,
                dependencies: resolved[i].clone(),
                state: None,
                attempts: 0,
            })
            .collect();

        Ok(Self {
            jobs: data,
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

    fn find(&self, id: &str) -> Option<usize> {
        self.jobs.iter().position(|entry| entry.job.id == id)
    }

    fn terminal(state: &State) -> bool {
        matches!(
            state,
            State::Succeeded | State::Failed | State::Blocked
        )
    }

    pub fn begin_cycle(&mut self, now: u64) -> Result<(), Error> {
        self.check_time(now)?;
        if self.cycle_started
            && self.jobs.iter().any(|entry| {
                matches!(entry.state, Some(State::Waiting { .. } | State::Running { .. }))
            })
        {
            return Err(Error::Busy);
        }

        for entry in &mut self.jobs {
            entry.state = Some(State::Waiting { ready_at: now });
            entry.attempts = 0;
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
        for (i, entry) in self.jobs.iter().enumerate() {
            let ready_at = match entry.state {
                Some(State::Waiting { ready_at }) if ready_at <= now => ready_at,
                _ => continue,
            };
            if !entry.dependencies.iter().all(|&dep| {
                matches!(self.jobs[dep].state, Some(State::Succeeded))
            }) {
                continue;
            }

            let effective = if self.aging_ms == 0 {
                entry.job.priority
            } else {
                let age = (now - ready_at) / self.aging_ms;
                (u64::from(entry.job.priority) + age).min(9) as u8
            };

            if best.map_or(true, |(best_i, best_priority)| {
                effective > best_priority || (effective == best_priority && i < best_i)
            }) {
                best = Some((i, effective));
            }
        }

        let result = if let Some((i, _)) = best {
            let entry = &mut self.jobs[i];
            entry.attempts += 1;
            let attempt = entry.attempts;
            entry.state = Some(State::Running { attempt });
            Some(Ticket {
                id: entry.job.id.clone(),
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
        if !self.cycle_started {
            return Err(Error::NoCycle);
        }
        let index = self.find(id).ok_or(Error::Unknown)?;
        if !matches!(self.jobs[index].state, Some(State::Running { .. })) {
            return Err(Error::NotRunning);
        }

        if success {
            self.jobs[index].state = Some(State::Succeeded);
        } else if self.jobs[index].attempts == self.jobs[index].job.max_attempts {
            self.jobs[index].state = Some(State::Failed);
        } else {
            let attempt = self.jobs[index].attempts;
            let mut delay = self.retry_base;
            for _ in 1..attempt {
                delay = delay.saturating_mul(2).min(self.retry_cap);
                if delay == self.retry_cap {
                    break;
                }
            }
            self.jobs[index].state = Some(State::Waiting {
                ready_at: now.saturating_add(delay),
            });
        }

        if matches!(self.jobs[index].state, Some(State::Failed)) {
            loop {
                let mut changed = false;
                for i in 0..self.jobs.len() {
                    if matches!(self.jobs[i].state, Some(State::Waiting { .. }))
                        && self.jobs[i].dependencies.iter().any(|&dep| {
                            matches!(
                                self.jobs[dep].state,
                                Some(State::Failed | State::Blocked)
                            )
                        })
                    {
                        self.jobs[i].state = Some(State::Blocked);
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
        let index = self.find(id).ok_or(Error::Unknown)?;
        self.jobs[index].state.clone().ok_or(Error::NoCycle)
    }
}