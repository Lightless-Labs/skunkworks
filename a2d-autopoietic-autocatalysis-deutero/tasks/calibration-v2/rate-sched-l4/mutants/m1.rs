#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Job {
    pub id: String,
    pub priority: u8,
    pub dependencies: Vec<String>,
    pub max_attempts: u8,
    pub release_at: u64,
    pub duration: u64,
    pub tag: Option<String>,
    pub failures: u8,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum State {
    Waiting { ready_at: u64, attempt: u8 },
    Running { attempt: u8, finish_at: u64 },
    Succeeded,
    Failed,
    Blocked,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Error {
    Config,
    BadId(usize),
    BadPriority(usize),
    BadAttempts(usize),
    BadTime(usize),
    BadTag(usize),
    Duplicate(usize),
    DuplicateDependency(usize, usize),
    MissingDependency(usize, usize),
    Cycle,
    TimeReversed,
    TimeRange,
    Unknown,
}
pub struct Pool {
    jobs: Vec<Job>,
    deps: Vec<Vec<usize>>,
    states: Vec<State>,
    remaining: Vec<Option<u64>>,
    started: Vec<u64>,
    capacity: usize,
    aging: u64,
    base: u64,
    cap: u64,
    horizon: u64,
    cursor: u64,
    initialized: bool,
}
impl Pool {
    pub fn new(
        jobs: Vec<Job>,
        capacity: usize,
        aging_ms: u64,
        retry_base: u64,
        retry_cap: u64,
    ) -> Result<Self, Error> {
        if capacity == 0
            || capacity > 16
            || retry_base == 0
            || retry_cap < retry_base
            || retry_cap > 1000000000
        {
            return Err(Error::Config);
        }
        let valid = |s: &str, max: usize| {
            !s.is_empty()
                && s.len() <= max
                && s.bytes()
                    .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
        };
        for (i, j) in jobs.iter().enumerate() {
            if !valid(&j.id, 16) {
                return Err(Error::BadId(i));
            }
            if j.priority > 9 {
                return Err(Error::BadPriority(i));
            }
            if j.max_attempts == 0 || j.max_attempts > 32 {
                return Err(Error::BadAttempts(i));
            }
            if j.release_at > 1000000000 || j.duration == 0 || j.duration > 1000000000 {
                return Err(Error::BadTime(i));
            }
            if j.tag.as_ref().is_some_and(|s| !valid(s, 8)) {
                return Err(Error::BadTag(i));
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
        let states = jobs
            .iter()
            .map(|j| State::Waiting {
                ready_at: j.release_at,
                attempt: 0,
            })
            .collect();
        Ok(Self {
            jobs,
            deps,
            states,
            remaining: vec![None; n],
            started: vec![0; n],
            capacity,
            aging: aging_ms,
            base: retry_base,
            cap: retry_cap,
            horizon: 0,
            cursor: 0,
            initialized: false,
        })
    }
    pub fn status(&self, id: &str) -> Result<State, Error> {
        let i = self
            .jobs
            .iter()
            .position(|j| j.id == id)
            .ok_or(Error::Unknown)?;
        Ok(self.states[i].clone())
    }
    fn phase(&mut self, t: u64, out: &mut Vec<String>) {
        let mut finishes: Vec<_> = (0..self.jobs.len())
            .filter(|i| matches!(self.states[*i],State::Running{finish_at,..} if finish_at==t))
            .collect();
        finishes.sort_by(|a, b| self.jobs[*a].id.cmp(&self.jobs[*b].id));
        for i in finishes {
            let State::Running { attempt, .. } = self.states[i] else {
                unreachable!()
            };
            let ok = attempt > self.jobs[i].failures;
            out.push(format!(
                "{t} finish {} {attempt} {}",
                self.jobs[i].id,
                if ok { "ok" } else { "fail" }
            ));
            if ok {
                self.states[i] = State::Succeeded;
            } else if attempt >= self.jobs[i].max_attempts {
                self.states[i] = State::Failed;
            } else {
                let delay = self
                    .base
                    .saturating_mul(1u64 << (attempt - 1))
                    .min(self.cap);
                let ready = t + delay;
                self.states[i] = State::Waiting {
                    ready_at: ready,
                    attempt,
                };
                out.push(format!("{t} retry {} {attempt} {ready}", self.jobs[i].id));
            }
            self.remaining[i] = None;
        }
        let mut skipped = Vec::new();
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
                skipped.push(i);
            }
        }
        skipped.sort_by(|a, b| self.jobs[*a].id.cmp(&self.jobs[*b].id));
        for i in skipped {
            out.push(format!("{t} skip {} dependency", self.jobs[i].id));
        }
        loop {
            let mut candidates = Vec::new();
            for i in 0..self.jobs.len() {
                let State::Waiting { ready_at, .. } = self.states[i] else {
                    continue;
                };
                if ready_at > t
                    || !self.deps[i]
                        .iter()
                        .all(|d| self.states[*d] == State::Succeeded)
                {
                    continue;
                }
                let age = if self.aging == 0 {
                    0
                } else {
                    (t - ready_at) / self.aging
                };
                let p = (self.jobs[i].priority as u64).saturating_add(age).min(9);
                candidates.push((std::cmp::Reverse(p), i));
            }
            candidates.sort_by(|a, b| a.0.cmp(&b.0).then_with(|| b.1.cmp(&a.1)));
            let running: Vec<_> = (0..self.jobs.len())
                .filter(|i| matches!(self.states[*i], State::Running { .. }))
                .collect();
            let mut choice = None;
            for (_, i) in candidates {
                let holder = running
                    .iter()
                    .copied()
                    .find(|v| self.jobs[i].tag.is_some() && self.jobs[*v].tag == self.jobs[i].tag);
                if running.len() < self.capacity && holder.is_none() {
                    choice = Some((i, None));
                    break;
                }
                let mut victims: Vec<_> = running
                    .iter()
                    .copied()
                    .filter(|v| {
                        self.jobs[i].priority as u16 >= self.jobs[*v].priority as u16 + 2
                            && holder.is_none_or(|h| h == *v)
                    })
                    .collect();
                victims.sort_by(|a, b| {
                    self.jobs[*a]
                        .priority
                        .cmp(&self.jobs[*b].priority)
                        .then_with(|| self.started[*b].cmp(&self.started[*a]))
                        .then_with(|| self.jobs[*b].id.cmp(&self.jobs[*a].id))
                });
                if let Some(v) = victims.first().copied() {
                    choice = Some((i, Some(v)));
                    break;
                }
            }
            let Some((i, victim)) = choice else {
                break;
            };
            if let Some(v) = victim {
                let State::Running { attempt, finish_at } = self.states[v] else {
                    unreachable!()
                };
                self.remaining[v] = Some(finish_at - t);
                self.states[v] = State::Waiting {
                    ready_at: t,
                    attempt,
                };
                out.push(format!(
                    "{t} preempt {} by {}",
                    self.jobs[v].id, self.jobs[i].id
                ));
            }
            let State::Waiting { attempt, .. } = self.states[i] else {
                unreachable!()
            };
            let (a, d) = if let Some(rem) = self.remaining[i].take() {
                (attempt, rem)
            } else {
                (attempt + 1, self.jobs[i].duration)
            };
            self.states[i] = State::Running {
                attempt: a,
                finish_at: t + d,
            };
            self.started[i] = t;
            out.push(format!("{t} start {} {a} {d}", self.jobs[i].id));
        }
    }
    pub fn run(&mut self, until: u64) -> Result<Vec<String>, Error> {
        if until < self.horizon {
            return Err(Error::TimeReversed);
        }
        if until > 1000000000 {
            return Err(Error::TimeRange);
        }
        let mut out = Vec::new();
        if !self.initialized {
            self.phase(0, &mut out);
            self.initialized = true;
        }
        loop {
            let mut next = None;
            for s in &self.states {
                let time = match s {
                    State::Running { finish_at, .. } => Some(*finish_at),
                    State::Waiting { ready_at, .. } if *ready_at > self.cursor => Some(*ready_at),
                    _ => None,
                };
                if let Some(t) = time {
                    if t > self.cursor && next.is_none_or(|n| t < n) {
                        next = Some(t);
                    }
                }
            }
            let Some(t) = next else {
                break;
            };
            if t > until {
                break;
            }
            self.cursor = t;
            self.phase(t, &mut out);
        }
        self.horizon = until;
        Ok(out)
    }
}
