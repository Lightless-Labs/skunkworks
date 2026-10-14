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
pub struct Runner {
    jobs: Vec<Job>,
    deps: Vec<Vec<usize>>,
    states: Vec<State>,
    attempts: Vec<u8>,
    active: bool,
    clock: u64,
    aging: u64,
    base: u64,
    cap: u64,
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
        for (i, j) in jobs.iter().enumerate() {
            if j.id.is_empty()
                || j.id.len() > 16
                || !j
                    .id
                    .bytes()
                    .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
            {
                return Err(Error::BadId(i));
            }
            if j.priority > 9 {
                return Err(Error::BadPriority(i));
            }
            if j.max_attempts == 0 || j.max_attempts > 32 {
                return Err(Error::BadAttempts(i));
            }
            if jobs[..i].iter().any(|x| x.id == j.id) {
                return Err(Error::Duplicate(i));
            }
        }
        let mut deps = Vec::new();
        for (i, j) in jobs.iter().enumerate() {
            let mut ds = Vec::new();
            for (k, id) in j.dependencies.iter().enumerate() {
                if j.dependencies[..k].contains(id) {
                    return Err(Error::DuplicateDependency(i, k));
                }
                ds.push(
                    jobs.iter()
                        .position(|x| &x.id == id)
                        .ok_or(Error::MissingDependency(i, k))?,
                );
            }
            deps.push(ds);
        }
        let mut done = vec![false; jobs.len()];
        for _ in 0..jobs.len() {
            let Some(i) = (0..jobs.len()).find(|i| !done[*i] && deps[*i].iter().all(|d| done[*d]))
            else {
                return Err(Error::Cycle);
            };
            done[i] = true;
        }
        let n = jobs.len();
        Ok(Self {
            jobs,
            deps,
            states: vec![State::Blocked; n],
            attempts: vec![0; n],
            active: false,
            clock: 0,
            aging: aging_ms,
            base: retry_base,
            cap: retry_cap,
        })
    }
    fn time(&self, now: u64) -> Result<(), Error> {
        if now < self.clock {
            Err(Error::TimeReversed)
        } else {
            Ok(())
        }
    }
    pub fn begin_cycle(&mut self, now: u64) -> Result<(), Error> {
        self.time(now)?;
        if self.active
            && self
                .states
                .iter()
                .any(|s| matches!(s, State::Waiting { .. } | State::Running { .. }))
        {
            return Err(Error::Busy);
        }
        self.states.fill(State::Waiting { ready_at: now });
        self.attempts.fill(0);
        self.active = true;
        self.clock = now;
        Ok(())
    }
    pub fn status(&self, id: &str) -> Result<State, Error> {
        if !self.active {
            return Err(Error::NoCycle);
        }
        let i = self
            .jobs
            .iter()
            .position(|j| j.id == id)
            .ok_or(Error::Unknown)?;
        Ok(self.states[i].clone())
    }
    pub fn next_ready(&mut self, now: u64) -> Result<Option<Ticket>, Error> {
        self.time(now)?;
        if !self.active {
            return Err(Error::NoCycle);
        }
        let mut best: Option<(usize, u8)> = None;
        for i in 0..self.jobs.len() {
            let State::Waiting { ready_at } = self.states[i] else {
                continue;
            };
            if ready_at > now
                || !self.deps[i]
                    .iter()
                    .all(|d| self.states[*d] == State::Succeeded)
            {
                continue;
            }
            let age = if self.aging == 0 {
                0
            } else {
                (now - ready_at) / self.aging
            };
            let p = (self.jobs[i].priority as u64).saturating_add(age).min(9) as u8;
            if best.is_none_or(|(_, bp)| p >= bp) {
                best = Some((i, p));
            }
        }
        self.clock = now;
        Ok(best.map(|(i, _)| {
            self.attempts[i] += 1;
            self.states[i] = State::Running {
                attempt: self.attempts[i],
            };
            Ticket {
                id: self.jobs[i].id.clone(),
                attempt: self.attempts[i],
            }
        }))
    }
    pub fn complete(&mut self, now: u64, id: &str, success: bool) -> Result<(), Error> {
        self.time(now)?;
        if !self.active {
            return Err(Error::NoCycle);
        }
        let i = self
            .jobs
            .iter()
            .position(|j| j.id == id)
            .ok_or(Error::Unknown)?;
        if !matches!(self.states[i], State::Running { .. }) {
            return Err(Error::NotRunning);
        }
        if success {
            self.states[i] = State::Succeeded;
        } else if self.attempts[i] >= self.jobs[i].max_attempts {
            self.states[i] = State::Failed;
        } else {
            let delay = self
                .base
                .saturating_mul(1u64 << (self.attempts[i] - 1))
                .min(self.cap);
            self.states[i] = State::Waiting {
                ready_at: now.saturating_add(delay),
            };
        }
        loop {
            let blocked: Vec<_> = (0..self.jobs.len())
                .filter(|i| {
                    matches!(self.states[*i], State::Waiting { .. })
                        && self.deps[*i]
                            .iter()
                            .any(|d| matches!(self.states[*d], State::Failed | State::Blocked))
                })
                .collect();
            if blocked.is_empty() {
                break;
            }
            for i in blocked {
                self.states[i] = State::Blocked;
            }
        }
        self.clock = now;
        Ok(())
    }
}
