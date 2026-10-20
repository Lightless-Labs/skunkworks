use std::collections::{HashMap, HashSet, VecDeque};

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

#[derive(Clone)]
struct Segment {
    remaining: Option<u64>,
    started_at: u64,
}

pub struct Pool {
    jobs: Vec<Job>,
    deps: Vec<Vec<usize>>,
    states: Vec<State>,
    segments: Vec<Segment>,
    ids: HashMap<String, usize>,
    capacity: usize,
    aging_ms: u64,
    retry_base: u64,
    retry_cap: u64,
    horizon: u64,
    has_run: bool,
}

impl Pool {
    pub fn new(
        jobs: Vec<Job>,
        capacity: usize,
        aging_ms: u64,
        retry_base: u64,
        retry_cap: u64,
    ) -> Result<Self, Error> {
        if !(1..=16).contains(&capacity)
            || retry_base < 1
            || retry_base > retry_cap
            || retry_cap > 1_000_000_000
        {
            return Err(Error::Config);
        }
        let mut ids = HashMap::new();
        for (i, j) in jobs.iter().enumerate() {
            if !valid_token(&j.id, 1, 16) {
                return Err(Error::BadId(i));
            }
            if j.priority > 9 {
                return Err(Error::BadPriority(i));
            }
            if !(1..=32).contains(&j.max_attempts) {
                return Err(Error::BadAttempts(i));
            }
            if j.release_at > 1_000_000_000
                || j.duration == 0
                || j.duration > 1_000_000_000
            {
                return Err(Error::BadTime(i));
            }
            if j.tag.as_ref().is_some_and(|s| !valid_token(s, 1, 8)) {
                return Err(Error::BadTag(i));
            }
            if ids.insert(j.id.clone(), i).is_some() {
                return Err(Error::Duplicate(i));
            }
        }

        let mut deps = vec![Vec::new(); jobs.len()];
        for (i, j) in jobs.iter().enumerate() {
            let mut seen = HashSet::new();
            for (k, name) in j.dependencies.iter().enumerate() {
                if !seen.insert(name.clone()) {
                    return Err(Error::DuplicateDependency(i, k));
                }
                let Some(&d) = ids.get(name) else {
                    return Err(Error::MissingDependency(i, k));
                };
                deps[i].push(d);
            }
        }

        let mut indeg = vec![0usize; jobs.len()];
        let mut reverse = vec![Vec::new(); jobs.len()];
        for (i, ds) in deps.iter().enumerate() {
            indeg[i] = ds.len();
            for &d in ds {
                reverse[d].push(i);
            }
        }
        let mut q = VecDeque::new();
        for (i, &d) in indeg.iter().enumerate() {
            if d == 0 {
                q.push_back(i);
            }
        }
        let mut count = 0;
        while let Some(i) = q.pop_front() {
            count += 1;
            for &child in &reverse[i] {
                indeg[child] -= 1;
                if indeg[child] == 0 {
                    q.push_back(child);
                }
            }
        }
        if count != jobs.len() {
            return Err(Error::Cycle);
        }

        let states = jobs
            .iter()
            .map(|j| State::Waiting {
                ready_at: j.release_at,
                attempt: 0,
            })
            .collect();
        let n = jobs.len();
        Ok(Self {
            jobs,
            deps,
            states,
            segments: vec![
                Segment {
                    remaining: None,
                    started_at: 0,
                };
                n
            ],
            ids,
            capacity,
            aging_ms,
            retry_base,
            retry_cap,
            horizon: 0,
            has_run: false,
        })
    }

    pub fn status(&self, id: &str) -> Result<State, Error> {
        self.ids
            .get(id)
            .map(|&i| self.states[i].clone())
            .ok_or(Error::Unknown)
    }

    pub fn run(&mut self, until: u64) -> Result<Vec<String>, Error> {
        if self.has_run && until < self.horizon {
            return Err(Error::TimeReversed);
        }
        if until > 1_000_000_000 {
            return Err(Error::TimeRange);
        }
        if self.has_run && until == self.horizon {
            return Ok(Vec::new());
        }

        let mut events = Vec::new();
        let mut time = if self.has_run { self.horizon } else { 0 };
        let mut first = !self.has_run;
        loop {
            if first {
                first = false;
            } else {
                let Some(next) = self.next_event(time, until) else {
                    break;
                };
                time = next;
            }
            self.process_time(time, &mut events);
            if time == until {
                break;
            }
        }
        self.horizon = until;
        self.has_run = true;
        Ok(events)
    }

    fn next_event(&self, after: u64, until: u64) -> Option<u64> {
        let mut best = None;
        for state in &self.states {
            let t = match state {
                State::Waiting { ready_at, .. } if *ready_at > after => *ready_at,
                State::Running { finish_at, .. } if *finish_at > after => *finish_at,
                _ => continue,
            };
            if t <= until && best.map_or(true, |b| t < b) {
                best = Some(t);
            }
        }
        best
    }

    fn process_time(&mut self, t: u64, out: &mut Vec<String>) {
        let mut done = Vec::new();
        for i in 0..self.jobs.len() {
            if matches!(self.states[i], State::Running { finish_at, .. } if finish_at == t) {
                done.push(i);
            }
        }
        done.sort_by(|&a, &b| self.jobs[a].id.cmp(&self.jobs[b].id));
        for i in done {
            let attempt = match self.states[i] {
                State::Running { attempt, .. } => attempt,
                _ => continue,
            };
            let success = attempt > self.jobs[i].failures;
            if success {
                self.states[i] = State::Succeeded;
                self.segments[i].remaining = None;
                out.push(format!("{} finish {} {} ok", t, self.jobs[i].id, attempt));
            } else {
                out.push(format!("{} finish {} {} fail", t, self.jobs[i].id, attempt));
                self.segments[i].remaining = None;
                if attempt == self.jobs[i].max_attempts {
                    self.states[i] = State::Failed;
                } else {
                    let ready = t.saturating_add(retry_delay(self.retry_base, self.retry_cap, attempt));
                    self.states[i] = State::Waiting {
                        ready_at: ready,
                        attempt,
                    };
                    out.push(format!("{} retry {} {} {}", t, self.jobs[i].id, attempt, ready));
                }
            }
        }

        let mut newly = Vec::new();
        loop {
            let mut changed = false;
            for i in 0..self.jobs.len() {
                if matches!(self.states[i], State::Waiting { .. }) {
                    if let Some(&dep) = self.deps[i]
                        .iter()
                        .find(|&&d| matches!(self.states[d], State::Failed | State::Blocked))
                    {
                        self.states[i] = State::Blocked;
                        newly.push((i, dep));
                        changed = true;
                    }
                }
            }
            if !changed {
                break;
            }
        }
        newly.sort_by(|(a, _), (b, _)| self.jobs[*a].id.cmp(&self.jobs[*b].id));
        for (i, d) in newly {
            out.push(format!("{} skip {} {}", t, self.jobs[i].id, self.jobs[d].id));
        }

        self.schedule(t, out);
    }

    fn schedule(&mut self, t: u64, out: &mut Vec<String>) {
        loop {
            let mut eligible: Vec<usize> = (0..self.jobs.len())
                .filter(|&i| {
                    matches!(self.states[i], State::Waiting { ready_at, .. } if ready_at <= t)
                        && self.deps[i]
                            .iter()
                            .all(|&d| matches!(self.states[d], State::Succeeded))
                })
                .collect();
            eligible.sort_by(|&a, &b| {
                self.effective(a, t)
                    .cmp(&self.effective(b, t))
                    .reverse()
                    .then_with(|| a.cmp(&b))
            });

            let mut action = None;
            for c in eligible {
                if self.direct(c) {
                    action = Some((c, None));
                    break;
                }
                if let Some(v) = self.victim(c) {
                    action = Some((c, Some(v)));
                    break;
                }
            }
            let Some((c, victim)) = action else {
                break;
            };
            if let Some(v) = victim {
                let (attempt, finish) = match self.states[v] {
                    State::Running { attempt, finish_at } => (attempt, finish_at),
                    _ => unreachable!(),
                };
                self.states[v] = State::Waiting {
                    ready_at: t,
                    attempt,
                };
                self.segments[v].remaining = Some(finish - t);
                out.push(format!("{} preempt {} by {}", t, self.jobs[v].id, self.jobs[c].id));
            }
            let (attempt, duration) = match self.states[c].clone() {
                State::Waiting { attempt, .. } => match self.segments[c].remaining.take() {
                    Some(r) => (attempt, r),
                    None => (attempt + 1, self.jobs[c].duration),
                },
                _ => continue,
            };
            let finish = t.saturating_add(duration);
            self.states[c] = State::Running {
                attempt,
                finish_at: finish,
            };
            self.segments[c].started_at = t;
            out.push(format!("{} start {} {} {}", t, self.jobs[c].id, attempt, duration));
        }
    }

    fn direct(&self, c: usize) -> bool {
        let count = self.states.iter().filter(|s| matches!(s, State::Running { .. })).count();
        if count >= self.capacity {
            return false;
        }
        match self.jobs[c].tag.as_deref() {
            None => true,
            Some(tag) => !(0..self.jobs.len()).any(|i| {
                matches!(self.states[i], State::Running { .. })
                    && self.jobs[i].tag.as_deref() == Some(tag)
            }),
        }
    }

    fn victim(&self, c: usize) -> Option<usize> {
        let n_running = self.states.iter().filter(|s| matches!(s, State::Running { .. })).count();
        let tag = self.jobs[c].tag.as_deref();
        let mut choices = Vec::new();
        for i in 0..self.jobs.len() {
            if !matches!(self.states[i], State::Running { .. }) {
                continue;
            }
            if self.jobs[c].priority < self.jobs[i].priority.saturating_add(2) {
                continue;
            }
            if n_running - 1 >= self.capacity {
                continue;
            }
            let tag_free = tag.is_none()
                || !(0..self.jobs.len()).any(|j| {
                    j != i
                        && matches!(self.states[j], State::Running { .. })
                        && self.jobs[j].tag.as_deref() == tag
                });
            if tag_free {
                choices.push(i);
            }
        }
        choices.sort_by(|&a, &b| {
            self.jobs[a]
                .priority
                .cmp(&self.jobs[b].priority)
                .then_with(|| self.segments[b].started_at.cmp(&self.segments[a].started_at))
                .then_with(|| self.jobs[b].id.cmp(&self.jobs[a].id))
        });
        choices.into_iter().next()
    }

    fn effective(&self, i: usize, t: u64) -> u8 {
        if self.aging_ms == 0 {
            return self.jobs[i].priority;
        }
        let ready = match self.states[i] {
            State::Waiting { ready_at, .. } => ready_at,
            _ => t,
        };
        let gained = ((t - ready) / self.aging_ms).min(9) as u8;
        self.jobs[i].priority.saturating_add(gained).min(9)
    }
}

fn valid_token(s: &str, min: usize, max: usize) -> bool {
    (min..=max).contains(&s.len())
        && s.bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
}

fn retry_delay(base: u64, cap: u64, attempt: u8) -> u64 {
    let mut delay = base;
    for _ in 1..attempt {
        if delay >= cap {
            return cap;
        }
        delay = delay.saturating_mul(2).min(cap);
    }
    delay.min(cap)
}
