use std::collections::HashSet;

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
struct RuntimeJob {
    job: Job,
    dependencies: Vec<usize>,
    state: State,
    order: usize,
    remaining: Option<u64>,
    segment_start: u64,
}

pub struct Pool {
    jobs: Vec<RuntimeJob>,
    capacity: usize,
    aging_ms: u64,
    retry_base: u64,
    retry_cap: u64,
    horizon: u64,
    has_run: bool,
}

fn valid_token(s: &str, min: usize, max: usize) -> bool {
    let b = s.as_bytes();
    (min..=max).contains(&b.len())
        && b.iter()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || *c == b'-')
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
            || retry_base == 0
            || retry_base > retry_cap
            || retry_cap > 1_000_000_000
        {
            return Err(Error::Config);
        }

        let mut ids = std::collections::HashMap::new();
        for (i, job) in jobs.iter().enumerate() {
            if !valid_token(&job.id, 1, 16) {
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
            if let Some(tag) = &job.tag {
                if !valid_token(tag, 1, 8) {
                    return Err(Error::BadTag(i));
                }
            }
            if ids.insert(job.id.clone(), i).is_some() {
                return Err(Error::Duplicate(i));
            }
        }

        let mut dependencies = Vec::with_capacity(jobs.len());
        for (i, job) in jobs.iter().enumerate() {
            let mut seen = HashSet::new();
            let mut deps = Vec::with_capacity(job.dependencies.len());
            for (j, name) in job.dependencies.iter().enumerate() {
                if !seen.insert(name.as_str()) {
                    return Err(Error::DuplicateDependency(i, j));
                }
                match ids.get(name) {
                    Some(&dep) => deps.push(dep),
                    None => return Err(Error::MissingDependency(i, j)),
                }
            }
            dependencies.push(deps);
        }

        fn visit(
            node: usize,
            deps: &[Vec<usize>],
            marks: &mut [u8],
        ) -> bool {
            if marks[node] == 1 {
                return false;
            }
            if marks[node] == 2 {
                return true;
            }
            marks[node] = 1;
            for &d in &deps[node] {
                if !visit(d, deps, marks) {
                    return false;
                }
            }
            marks[node] = 2;
            true
        }

        let mut marks = vec![0; jobs.len()];
        for i in 0..jobs.len() {
            if !visit(i, &dependencies, &mut marks) {
                return Err(Error::Cycle);
            }
        }

        let runtime = jobs
            .into_iter()
            .enumerate()
            .map(|(order, job)| RuntimeJob {
                state: State::Waiting {
                    ready_at: job.release_at,
                    attempt: 0,
                },
                job,
                dependencies: dependencies[order].clone(),
                order,
                remaining: None,
                segment_start: 0,
            })
            .collect();

        Ok(Self {
            jobs: runtime,
            capacity,
            aging_ms,
            retry_base,
            retry_cap,
            horizon: 0,
            has_run: false,
        })
    }

    pub fn status(&self, id: &str) -> Result<State, Error> {
        self.jobs
            .iter()
            .find(|j| j.job.id == id)
            .map(|j| j.state.clone())
            .ok_or(Error::Unknown)
    }

    pub fn run(&mut self, until: u64) -> Result<Vec<String>, Error> {
        if until < self.horizon {
            return Err(Error::TimeReversed);
        }
        if until > 1_000_000_000 {
            return Err(Error::TimeRange);
        }

        let mut lines = Vec::new();
        let mut t = if self.has_run { self.horizon } else { 0 };
        let mut process_current = !self.has_run || until > self.horizon;

        while process_current && t <= until {
            let mut completions: Vec<usize> = self
                .jobs
                .iter()
                .enumerate()
                .filter_map(|(i, j)| match j.state {
                    State::Running { finish_at, .. } if finish_at == t => Some(i),
                    _ => None,
                })
                .collect();
            completions.sort_by(|&a, &b| self.jobs[a].job.id.cmp(&self.jobs[b].job.id));

            for i in completions {
                let attempt = match self.jobs[i].state {
                    State::Running { attempt, .. } => attempt,
                    _ => continue,
                };
                let ok = attempt > self.jobs[i].job.failures;
                if ok {
                    lines.push(format!("{} finish {} {} ok", t, self.jobs[i].job.id, attempt));
                    self.jobs[i].state = State::Succeeded;
                } else {
                    lines.push(format!("{} finish {} {} fail", t, self.jobs[i].job.id, attempt));
                    if attempt >= self.jobs[i].job.max_attempts {
                        self.jobs[i].state = State::Failed;
                    } else {
                        let shift = u32::from(attempt - 1);
                        let delay = self.retry_base.saturating_mul(1u64.checked_shl(shift).unwrap_or(u64::MAX)).min(self.retry_cap);
                        let ready = t.saturating_add(delay);
                        lines.push(format!("{} retry {} {} {}", t, self.jobs[i].job.id, attempt, ready));
                        self.jobs[i].state = State::Waiting {
                            ready_at: ready,
                            attempt,
                        };
                        self.jobs[i].remaining = None;
                    }
                }
            }

            loop {
                let mut newly_blocked = Vec::new();
                for i in 0..self.jobs.len() {
                    if !matches!(self.jobs[i].state, State::Waiting { .. }) {
                        continue;
                    }
                    if let Some(&dep) = self.jobs[i]
                        .dependencies
                        .iter()
                        .find(|&&d| matches!(self.jobs[d].state, State::Failed | State::Blocked))
                    {
                        newly_blocked.push((i, dep));
                    }
                }
                if newly_blocked.is_empty() {
                    break;
                }
                newly_blocked.sort_by(|(a, _), (b, _)| self.jobs[*a].job.id.cmp(&self.jobs[*b].job.id));
                for (i, dep) in newly_blocked {
                    if matches!(self.jobs[i].state, State::Waiting { .. }) {
                        lines.push(format!(
                            "{} skip {} {}",
                            t, self.jobs[i].job.id, self.jobs[dep].job.id
                        ));
                        self.jobs[i].state = State::Blocked;
                    }
                }
            }

            loop {
                let mut candidates: Vec<usize> = self
                    .jobs
                    .iter()
                    .enumerate()
                    .filter_map(|(i, j)| {
                        let State::Waiting { ready_at, .. } = j.state else {
                            return None;
                        };
                        (ready_at <= t
                            && j.dependencies.iter().all(|&d| {
                                matches!(self.jobs[d].state, State::Succeeded)
                            }))
                        .then_some(i)
                    })
                    .collect();

                candidates.sort_by(|&a, &b| {
                    let priority = |i: usize| {
                        let base = self.jobs[i].job.priority as u64;
                        if self.aging_ms == 0 {
                            base
                        } else {
                            let age_start = match self.jobs[i].state {
                                State::Waiting { ready_at, .. } => ready_at,
                                _ => t,
                            };
                            (base + (t - age_start) / self.aging_ms).min(9)
                        }
                    };
                    priority(b)
                        .cmp(&priority(a))
                        .then_with(|| self.jobs[a].order.cmp(&self.jobs[b].order))
                });

                let mut chosen = None;
                for &candidate in &candidates {
                    let tag = self.jobs[candidate].job.tag.as_deref();
                    let running: Vec<usize> = self
                        .jobs
                        .iter()
                        .enumerate()
                        .filter_map(|(i, j)| matches!(j.state, State::Running { .. }).then_some(i))
                        .collect();
                    let occupied = |i: usize| {
                        tag.is_some() && self.jobs[i].job.tag.as_deref() == tag
                    };
                    let slots_free = running.len() < self.capacity;
                    let tag_free = !running.iter().any(|&i| occupied(i));

                    if slots_free && tag_free {
                        chosen = Some((candidate, None));
                        break;
                    }

                    let mut victims: Vec<usize> = running
                        .iter()
                        .copied()
                        .filter(|&v| {
                            self.jobs[candidate].job.priority
                                >= self.jobs[v].job.priority.saturating_add(2)
                                && running.len() - 1 < self.capacity
                                && !running
                                    .iter()
                                    .copied()
                                    .filter(|&x| x != v)
                                    .any(occupied)
                        })
                        .collect();
                    victims.sort_by(|&a, &b| {
                        self.jobs[a]
                            .job
                            .priority
                            .cmp(&self.jobs[b].job.priority)
                            .then_with(|| self.jobs[b].segment_start.cmp(&self.jobs[a].segment_start))
                            .then_with(|| self.jobs[b].job.id.cmp(&self.jobs[a].job.id))
                    });
                    if let Some(victim) = victims.first().copied() {
                        chosen = Some((candidate, Some(victim)));
                        break;
                    }
                }

                let Some((candidate, victim)) = chosen else {
                    break;
                };

                if let Some(v) = victim {
                    let finish_at = match self.jobs[v].state {
                        State::Running { finish_at, .. } => finish_at,
                        _ => t,
                    };
                    let attempt = match self.jobs[v].state {
                        State::Running { attempt, .. } => attempt,
                        _ => 0,
                    };
                    let remaining = finish_at - t;
                    lines.push(format!(
                        "{} preempt {} by {}",
                        t, self.jobs[v].job.id, self.jobs[candidate].job.id
                    ));
                    self.jobs[v].state = State::Waiting {
                        ready_at: t,
                        attempt,
                    };
                    self.jobs[v].remaining = Some(remaining);
                }

                let (attempt, ready_at) = match self.jobs[candidate].state {
                    State::Waiting { ready_at, attempt } => (attempt, ready_at),
                    _ => unreachable!(),
                };
                let remaining = self.jobs[candidate].remaining.take().unwrap_or(self.jobs[candidate].job.duration);
                let next_attempt = if remaining == self.jobs[candidate].job.duration
                    && attempt < self.jobs[candidate].job.max_attempts
                {
                    attempt + 1
                } else {
                    attempt
                };
                let _ = ready_at;
                let finish_at = t.saturating_add(remaining);
                self.jobs[candidate].segment_start = t;
                self.jobs[candidate].state = State::Running {
                    attempt: next_attempt,
                    finish_at,
                };
                lines.push(format!(
                    "{} start {} {} {}",
                    t, self.jobs[candidate].job.id, next_attempt, remaining
                ));
            }

            let next = self
                .jobs
                .iter()
                .filter_map(|j| match j.state {
                    State::Running { finish_at, .. } if finish_at > t => Some(finish_at),
                    State::Waiting { ready_at, .. } if ready_at > t => Some(ready_at),
                    _ => None,
                })
                .min();
            match next {
                Some(next_time) if next_time <= until => {
                    t = next_time;
                    process_current = true;
                }
                _ => break,
            }
        }

        self.horizon = until;
        self.has_run = true;
        Ok(lines)
    }
}

fn main() {}