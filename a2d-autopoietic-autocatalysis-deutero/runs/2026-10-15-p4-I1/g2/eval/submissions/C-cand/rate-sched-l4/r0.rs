use std::collections::{HashMap, HashSet};

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
struct Runtime {
    state: State,
    remaining: Option<u64>,
    started_at: u64,
    order: usize,
}

pub struct Pool {
    jobs: Vec<Job>,
    ids: HashMap<String, usize>,
    dependencies: Vec<Vec<usize>>,
    runtime: Vec<Runtime>,
    capacity: usize,
    aging_ms: u64,
    retry_base: u64,
    retry_cap: u64,
    horizon: u64,
    first_run: bool,
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
        for (i, job) in jobs.iter().enumerate() {
            if !valid_name(&job.id, 1, 16) {
                return Err(Error::BadId(i));
            }
            if job.priority > 9 {
                return Err(Error::BadPriority(i));
            }
            if !(1..=32).contains(&job.max_attempts) {
                return Err(Error::BadAttempts(i));
            }
            if job.release_at > 1_000_000_000
                || job.duration == 0
                || job.duration > 1_000_000_000
            {
                return Err(Error::BadTime(i));
            }
            if job
                .tag
                .as_ref()
                .is_some_and(|tag| !valid_name(tag, 1, 8))
            {
                return Err(Error::BadTag(i));
            }
            if ids.contains_key(&job.id) {
                return Err(Error::Duplicate(i));
            }
            ids.insert(job.id.clone(), i);
        }

        let mut dependencies = Vec::with_capacity(jobs.len());
        for (i, job) in jobs.iter().enumerate() {
            let mut seen = HashSet::new();
            let mut deps = Vec::with_capacity(job.dependencies.len());
            for (j, spelling) in job.dependencies.iter().enumerate() {
                if !seen.insert(spelling) {
                    return Err(Error::DuplicateDependency(i, j));
                }
                match ids.get(spelling) {
                    Some(&dep) => deps.push(dep),
                    None => return Err(Error::MissingDependency(i, j)),
                }
            }
            dependencies.push(deps);
        }

        let mut marks = vec![0u8; jobs.len()];
        fn visit(node: usize, deps: &[Vec<usize>], marks: &mut [u8]) -> bool {
            if marks[node] == 1 {
                return false;
            }
            if marks[node] == 2 {
                return true;
            }
            marks[node] = 1;
            for &dep in &deps[node] {
                if !visit(dep, deps, marks) {
                    return false;
                }
            }
            marks[node] = 2;
            true
        }
        for i in 0..jobs.len() {
            if !visit(i, &dependencies, &mut marks) {
                return Err(Error::Cycle);
            }
        }

        let runtime = jobs
            .iter()
            .enumerate()
            .map(|(order, job)| Runtime {
                state: State::Waiting {
                    ready_at: job.release_at,
                    attempt: 0,
                },
                remaining: None,
                started_at: 0,
                order,
            })
            .collect();

        Ok(Self {
            jobs,
            ids,
            dependencies,
            runtime,
            capacity,
            aging_ms,
            retry_base,
            retry_cap,
            horizon: 0,
            first_run: true,
        })
    }

    pub fn status(&self, id: &str) -> Result<State, Error> {
        let &index = self.ids.get(id).ok_or(Error::Unknown)?;
        Ok(self.runtime[index].state.clone())
    }

    pub fn run(&mut self, until: u64) -> Result<Vec<String>, Error> {
        if until < self.horizon {
            return Err(Error::TimeReversed);
        }
        if until > 1_000_000_000 {
            return Err(Error::TimeRange);
        }

        let mut lines = Vec::new();
        let mut now = if self.first_run { 0 } else { self.horizon };
        let mut do_phase = self.first_run;
        self.first_run = false;

        loop {
            if do_phase {
                self.process_timestamp(now, &mut lines);
                do_phase = false;
            }

            let next = self.next_event_time(now);
            match next {
                Some(t) if t <= until => {
                    now = t;
                    do_phase = true;
                }
                _ => break,
            }
        }

        self.horizon = until;
        Ok(lines)
    }

    fn next_event_time(&self, now: u64) -> Option<u64> {
        let mut next: Option<u64> = None;
        let mut consider = |t: u64| {
            if t > now && next.is_none_or(|old| t < old) {
                next = Some(t);
            }
        };
        for (i, rt) in self.runtime.iter().enumerate() {
            match rt.state {
                State::Waiting { ready_at, .. } => consider(ready_at),
                State::Running { finish_at, .. } => consider(finish_at),
                _ => {
                    let _ = i;
                }
            }
        }
        next
    }

    fn process_timestamp(&mut self, now: u64, lines: &mut Vec<String>) {
        let mut completions = Vec::new();
        for (i, rt) in self.runtime.iter().enumerate() {
            if matches!(rt.state, State::Running { finish_at, .. } if finish_at == now) {
                completions.push(i);
            }
        }
        completions.sort_by(|&a, &b| self.jobs[a].id.cmp(&self.jobs[b].id));

        for i in completions {
            let attempt = match self.runtime[i].state {
                State::Running { attempt, .. } => attempt,
                _ => continue,
            };
            let ok = attempt > self.jobs[i].failures;
            if ok {
                self.runtime[i].state = State::Succeeded;
                self.runtime[i].remaining = None;
                lines.push(format!("{} finish {} {} ok", now, self.jobs[i].id, attempt));
            } else {
                lines.push(format!("{} finish {} {} fail", now, self.jobs[i].id, attempt));
                self.runtime[i].remaining = None;
                if attempt >= self.jobs[i].max_attempts {
                    self.runtime[i].state = State::Failed;
                } else {
                    let shift = u32::from(attempt - 1);
                    let delay = self
                        .retry_base
                        .checked_mul(1u64.checked_shl(shift).unwrap_or(u64::MAX))
                        .unwrap_or(u64::MAX)
                        .min(self.retry_cap);
                    let ready = now.saturating_add(delay);
                    self.runtime[i].state = State::Waiting {
                        ready_at: ready,
                        attempt,
                    };
                    lines.push(format!("{} retry {} {} {}", now, self.jobs[i].id, attempt, ready));
                }
            }
        }

        loop {
            let mut newly_blocked = Vec::new();
            for i in 0..self.jobs.len() {
                if !matches!(self.runtime[i].state, State::Waiting { .. }) {
                    continue;
                }
                if let Some(&dep) = self.dependencies[i].iter().find(|&&dep| {
                    matches!(self.runtime[dep].state, State::Failed | State::Blocked)
                }) {
                    newly_blocked.push((i, dep));
                }
            }
            if newly_blocked.is_empty() {
                break;
            }
            for &(i, _) in &newly_blocked {
                self.runtime[i].state = State::Blocked;
            }
            newly_blocked.sort_by(|(a, _), (b, _)| self.jobs[*a].id.cmp(&self.jobs[*b].id));
            for (i, dep) in newly_blocked {
                lines.push(format!("{} skip {} {}", now, self.jobs[i].id, self.jobs[dep].id));
            }
        }

        loop {
            let mut candidates = Vec::new();
            for i in 0..self.jobs.len() {
                let State::Waiting { ready_at, .. } = self.runtime[i].state else {
                    continue;
                };
                if ready_at > now
                    || self.dependencies[i]
                        .iter()
                        .any(|&dep| !matches!(self.runtime[dep].state, State::Succeeded))
                {
                    continue;
                }
                let age = if self.aging_ms == 0 {
                    0
                } else {
                    (now - ready_at) / self.aging_ms
                };
                let priority = (u64::from(self.jobs[i].priority) + age).min(9);
                candidates.push((i, priority));
            }
            candidates.sort_by(|(a, pa), (b, pb)| {
                pb.cmp(pa).then_with(|| self.runtime[*a].order.cmp(&self.runtime[*b].order))
            });

            let mut dispatched = false;
            for (candidate, _) in candidates {
                let tag = self.jobs[candidate].tag.as_deref();
                let running: Vec<usize> = self
                    .runtime
                    .iter()
                    .enumerate()
                    .filter_map(|(i, rt)| matches!(rt.state, State::Running { .. }).then_some(i))
                    .collect();
                let slot_free = running.len() < self.capacity;
                let tag_free = tag.is_none()
                    || !running
                        .iter()
                        .any(|&i| self.jobs[i].tag.as_deref() == tag);

                if slot_free && tag_free {
                    self.start(candidate, now, lines);
                    dispatched = true;
                    break;
                }

                let mut victims: Vec<usize> = running
                    .iter()
                    .copied()
                    .filter(|&victim| {
                        self.jobs[candidate].priority
                            >= self.jobs[victim].priority.saturating_add(2)
                    })
                    .filter(|&victim| {
                        let after_count = running.len() - 1;
                        let slot_ok = after_count < self.capacity;
                        let tag_ok = tag.is_none()
                            || (self.jobs[victim].tag.as_deref() == tag
                                && !running.iter().any(|&other| {
                                    other != victim && self.jobs[other].tag.as_deref() == tag
                                }));
                        slot_ok && tag_ok
                    })
                    .collect();
                victims.sort_by(|&a, &b| {
                    self.jobs[a]
                        .priority
                        .cmp(&self.jobs[b].priority)
                        .then_with(|| self.runtime[b].started_at.cmp(&self.runtime[a].started_at))
                        .then_with(|| self.jobs[b].id.cmp(&self.jobs[a].id))
                });
                if let Some(victim) = victims.first().copied() {
                    let (attempt, finish_at) = match self.runtime[victim].state {
                        State::Running { attempt, finish_at } => (attempt, finish_at),
                        _ => unreachable!(),
                    };
                    let remaining = finish_at - now;
                    lines.push(format!(
                        "{} preempt {} by {}",
                        now, self.jobs[victim].id, self.jobs[candidate].id
                    ));
                    self.runtime[victim].state = State::Waiting {
                        ready_at: now,
                        attempt,
                    };
                    self.runtime[victim].remaining = Some(remaining);
                    self.start(candidate, now, lines);
                    dispatched = true;
                    break;
                }
            }
            if !dispatched {
                break;
            }
        }
    }

    fn start(&mut self, index: usize, now: u64, lines: &mut Vec<String>) {
        let (attempt, remaining) = match self.runtime[index].state {
            State::Waiting { attempt, .. } => {
                if let Some(remaining) = self.runtime[index].remaining.take() {
                    (attempt, remaining)
                } else {
                    (attempt.saturating_add(1), self.jobs[index].duration)
                }
            }
            _ => unreachable!(),
        };
        let finish_at = now.saturating_add(remaining);
        self.runtime[index].started_at = now;
        self.runtime[index].state = State::Running { attempt, finish_at };
        lines.push(format!(
            "{} start {} {} {}",
            now, self.jobs[index].id, attempt, remaining
        ));
    }
}

fn valid_name(s: &str, min: usize, max: usize) -> bool {
    let b = s.as_bytes();
    (min..=max).contains(&b.len())
        && b.iter()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || *c == b'-')
}

fn main() {}