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

struct Entry {
    job: Job,
    deps: Vec<usize>,
    state: State,
    attempts: u8,
}

pub struct Runner {
    entries: Vec<Entry>,
    aging: u64,
    base: u64,
    cap: u64,
    last: u64,
    active: bool,
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

        let mut lookup = std::collections::HashMap::new();
        for (i, j) in jobs.iter().enumerate() {
            if !valid_id(&j.id) {
                return Err(Error::BadId(i));
            }
            if j.priority > 9 {
                return Err(Error::BadPriority(i));
            }
            if !(1..=32).contains(&j.max_attempts) {
                return Err(Error::BadAttempts(i));
            }
            if lookup.insert(j.id.clone(), i).is_some() {
                return Err(Error::Duplicate(i));
            }
        }

        let mut deps = Vec::new();
        for (i, j) in jobs.iter().enumerate() {
            let mut seen = std::collections::HashSet::new();
            let mut row = Vec::new();
            for (k, name) in j.dependencies.iter().enumerate() {
                if !seen.insert(name) {
                    return Err(Error::DuplicateDependency(i, k));
                }
                match lookup.get(name) {
                    Some(&d) => row.push(d),
                    None => return Err(Error::MissingDependency(i, k)),
                }
            }
            deps.push(row);
        }

        let mut indegree = vec![0usize; jobs.len()];
        let mut children = vec![Vec::new(); jobs.len()];
        for (i, row) in deps.iter().enumerate() {
            for &d in row {
                indegree[i] += 1;
                children[d].push(i);
            }
        }
        let mut queue: std::collections::VecDeque<usize> = indegree
            .iter()
            .enumerate()
            .filter_map(|(i, &n)| (n == 0).then_some(i))
            .collect();
        let mut visited = 0;
        while let Some(i) = queue.pop_front() {
            visited += 1;
            for &child in &children[i] {
                indegree[child] -= 1;
                if indegree[child] == 0 {
                    queue.push_back(child);
                }
            }
        }
        if visited != jobs.len() {
            return Err(Error::Cycle);
        }

        let entries = jobs
            .into_iter()
            .enumerate()
            .map(|(i, job)| Entry {
                job,
                deps: deps[i].clone(),
                state: State::Waiting { ready_at: 0 },
                attempts: 0,
            })
            .collect();
        Ok(Self {
            entries,
            aging: aging_ms,
            base: retry_base,
            cap: retry_cap,
            last: 0,
            active: false,
        })
    }

    pub fn begin_cycle(&mut self, now: u64) -> Result<(), Error> {
        self.check(now)?;
        if self.active
            && self.entries.iter().any(|e| matches!(e.state, State::Waiting { .. } | State::Running { .. }))
        {
            return Err(Error::Busy);
        }
        for e in &mut self.entries {
            e.state = State::Waiting { ready_at: now };
            e.attempts = 0;
        }
        self.active = true;
        self.last = now;
        Ok(())
    }

    pub fn next_ready(&mut self, now: u64) -> Result<Option<Ticket>, Error> {
        self.check(now)?;
        if !self.active {
            return Err(Error::NoCycle);
        }
        let mut selected = None;
        for i in 0..self.entries.len() {
            let e = &self.entries[i];
            let start = match e.state {
                State::Waiting { ready_at } if ready_at <= now => ready_at,
                _ => continue,
            };
            if e.deps.iter().any(|&d| !matches!(self.entries[d].state, State::Succeeded)) {
                continue;
            }
            let age = if self.aging == 0 { 0 } else { (now - start) / self.aging };
            let priority = ((e.job.priority as u64).saturating_add(age)).min(9) as u8;
            if selected.map_or(true, |(best, p)| priority > p || (priority == p && i < best)) {
                selected = Some((i, priority));
            }
        }
        self.last = now;
        if let Some((i, _)) = selected {
            let e = &mut self.entries[i];
            e.attempts += 1;
            e.state = State::Running { attempt: e.attempts };
            Ok(Some(Ticket { id: e.job.id.clone(), attempt: e.attempts }))
        } else {
            Ok(None)
        }
    }

    pub fn complete(&mut self, now: u64, id: &str, success: bool) -> Result<(), Error> {
        self.check(now)?;
        if !self.active {
            return Err(Error::NoCycle);
        }
        let i = match self.entries.iter().position(|e| e.job.id == id) {
            Some(i) => i,
            None => return Err(Error::Unknown),
        };
        if !matches!(self.entries[i].state, State::Running { .. }) {
            return Err(Error::NotRunning);
        }

        self.last = now;
        if success {
            self.entries[i].state = State::Succeeded;
        } else if self.entries[i].attempts >= self.entries[i].job.max_attempts {
            self.entries[i].state = State::Failed;
            self.block_descendants();
        } else {
            let delay = retry_delay(self.base, self.cap, self.entries[i].attempts);
            self.entries[i].state = State::Waiting { ready_at: now.saturating_add(delay) };
        }
        Ok(())
    }

    pub fn status(&self, id: &str) -> Result<State, Error> {
        if !self.active {
            return Err(Error::NoCycle);
        }
        self.entries.iter().find(|e| e.job.id == id).map(|e| e.state.clone()).ok_or(Error::Unknown)
    }

    fn check(&self, now: u64) -> Result<(), Error> {
        if now < self.last { Err(Error::TimeReversed) } else { Ok(()) }
    }

    fn block_descendants(&mut self) {
        loop {
            let mut changed = false;
            for i in 0..self.entries.len() {
                if !matches!(self.entries[i].state, State::Waiting { .. }) {
                    continue;
                }
                let bad = self.entries[i].deps.iter().any(|&d| {
                    matches!(self.entries[d].state, State::Failed | State::Blocked)
                });
                if bad {
                    self.entries[i].state = State::Blocked;
                    changed = true;
                }
            }
            if !changed {
                return;
            }
        }
    }
}

fn valid_id(id: &str) -> bool {
    !id.is_empty()
        && id.len() <= 16
        && id.bytes().all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
}

fn retry_delay(base: u64, cap: u64, attempt: u8) -> u64 {
    let mut value = base;
    for _ in 1..attempt {
        value = value.saturating_mul(2).min(cap);
    }
    value.min(cap)
}
