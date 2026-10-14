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
    age_origin: u64,
    remaining: Option<u64>,
    started_at: u64,
}

pub struct Pool {
    jobs: Vec<Job>,
    runtime: Vec<Runtime>,
    dependencies: Vec<Vec<usize>>,
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

        let mut ids = std::collections::HashMap::<String, usize>::new();
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
                || !(1..=1_000_000_000).contains(&job.duration)
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
            let mut seen = std::collections::HashSet::new();
            let mut deps = Vec::with_capacity(job.dependencies.len());
            for (j, spelling) in job.dependencies.iter().enumerate() {
                if !seen.insert(spelling.as_str()) {
                    return Err(Error::DuplicateDependency(i, j));
                }
                match ids.get(spelling) {
                    Some(&index) => deps.push(index),
                    None => return Err(Error::MissingDependency(i, j)),
                }
            }
            dependencies.push(deps);
        }

        fn visit(
            node: usize,
            dependencies: &[Vec<usize>],
            marks: &mut [u8],
        ) -> bool {
            if marks[node] == 1 {
                return false;
            }
            if marks[node] == 2 {
                return true;
            }
            marks[node] = 1;
            for &dep in &dependencies[node] {
                if !visit(dep, dependencies, marks) {
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
            .iter()
            .map(|job| Runtime {
                state: State::Waiting {
                    ready_at: job.release_at,
                    attempt: 0,
                },
                age_origin: job.release_at,
                remaining: None,
                started_at: 0,
            })
            .collect();

        Ok(Self {
            jobs,
            runtime,
            dependencies,
            capacity,
            aging_ms,
            retry_base,
            retry_cap,
            horizon: 0,
            first_run: true,
        })
    }

    pub fn status(&self, id: &str) -> Result<State, Error> {
        self.jobs
            .iter()
            .position(|job| job.id == id)
            .map(|i| self.runtime[i].state.clone())
            .ok_or(Error::Unknown)
    }

    pub fn run(&mut self, until: u64) -> Result<Vec<String>, Error> {
        if until < self.horizon {
            return Err(Error::TimeReversed);
        }
        if until > 1_000_000_000 {
            return Err(Error::TimeRange);
        }

        let mut events = Vec::new();
        let mut time = if self.first_run { 0 } else { self.horizon };
        let mut first_phase = self.first_run;

        loop {
            let should_process = first_phase || time > self.horizon || self.first_run;
            if should_process {
                self.process_time(time, &mut events);
            }

            if time == until {
                break;
            }

            let mut next: Option<u64> = None;
            for runtime in &self.runtime {
                let t = match runtime.state {
                    State::Running { finish_at, .. } => Some(finish_at),
                    State::Waiting { ready_at, .. } => Some(ready_at),
                    _ => None,
                };
                if let Some(t) = t {
                    if t > time && t <= until && next.map_or(true, |n| t < n) {
                        next = Some(t);
                    }
                }
            }

            match next {
                Some(t) => {
                    time = t;
                    first_phase = true;
                }
                None => break,
            }
        }

        self.horizon = until;
        self.first_run = false;
        Ok(events)
    }

    fn process_time(&mut self, time: u64, events: &mut Vec<String>) {
        let mut completed: Vec<usize> = self
            .runtime
            .iter()
            .enumerate()
            .filter_map(|(i, rt)| match rt.state {
                State::Running { finish_at, .. } if finish_at == time => Some(i),
                _ => None,
            })
            .collect();
        completed.sort_by(|&a, &b| self.jobs[a].id.cmp(&self.jobs[b].id));

        for i in completed {
            let attempt = match self.runtime[i].state {
                State::Running { attempt, .. } => attempt,
                _ => continue,
            };
            let ok = attempt > self.jobs[i].failures;
            events.push(format!(
                "{} finish {} {} {}",
                time,
                self.jobs[i].id,
                attempt,
                if ok { "ok" } else { "fail" }
            ));
            self.runtime[i].remaining = None;
            if ok {
                self.runtime[i].state = State::Succeeded;
            } else if attempt >= self.jobs[i].max_attempts {
                self.runtime[i].state = State::Failed;
            } else {
                let shift = u32::from(attempt.saturating_sub(1));
                let delay = self
                    .retry_base
                    .checked_mul(1u64.checked_shl(shift).unwrap_or(u64::MAX))
                    .unwrap_or(u64::MAX)
                    .min(self.retry_cap);
                let ready = time.saturating_add(delay);
                self.runtime[i].state = State::Waiting {
                    ready_at: ready,
                    attempt,
                };
                self.runtime[i].age_origin = ready;
                events.push(format!(
                    "{} retry {} {} {}",
                    time, self.jobs[i].id, attempt, ready
                ));
            }
        }

        loop {
            let mut newly_blocked = Vec::new();
            for i in 0..self.jobs.len() {
                if matches!(self.runtime[i].state, State::Waiting { .. })
                    && self.dependencies[i].iter().any(|&d| {
                        matches!(
                            self.runtime[d].state,
                            State::Failed | State::Blocked
                        )
                    })
                {
                    newly_blocked.push(i);
                }
            }
            if newly_blocked.is_empty() {
                break;
            }
            newly_blocked.sort_by(|&a, &b| self.jobs[a].id.cmp(&self.jobs[b].id));
            for &i in &newly_blocked {
                let dep = self.dependencies[i]
                    .iter()
                    .filter(|&&d| {
                        matches!(
                            self.runtime[d].state,
                            State::Failed | State::Blocked
                        )
                    })
                    .map(|&d| self.jobs[d].id.as_str())
                    .min()
                    .unwrap_or("");
                events.push(format!("{} skip {} {}", time, self.jobs[i].id, dep));
                self.runtime[i].state = State::Blocked;
            }
        }

        loop {
            let mut candidates: Vec<usize> = (0..self.jobs.len())
                .filter(|&i| {
                    matches!(
                        self.runtime[i].state,
                        State::Waiting { ready_at, .. } if ready_at <= time
                    ) && self.dependencies[i]
                        .iter()
                        .all(|&d| matches!(self.runtime[d].state, State::Succeeded))
                })
                .collect();

            candidates.sort_by(|&a, &b| {
                let pa = self.effective_priority(a, time);
                let pb = self.effective_priority(b, time);
                pb.cmp(&pa).then(a.cmp(&b))
            });

            let mut selected = None;
            for candidate in candidates {
                let tag = self.jobs[candidate].tag.as_deref();
                let running: Vec<usize> = (0..self.jobs.len())
                    .filter(|&i| matches!(self.runtime[i].state, State::Running { .. }))
                    .collect();
                let tag_busy = tag.is_some_and(|t| {
                    running.iter().any(|&i| self.jobs[i].tag.as_deref() == Some(t))
                });

                if running.len() < self.capacity && !tag_busy {
                    selected = Some((candidate, None));
                    break;
                }

                let mut victims: Vec<usize> = running
                    .iter()
                    .copied()
                    .filter(|&v| {
                        self.jobs[candidate].priority >= self.jobs[v].priority.saturating_add(2)
                    })
                    .filter(|&v| {
                        let after_count = running.len() - 1;
                        let slot = after_count < self.capacity;
                        let tag_free = tag.map_or(true, |t| {
                            !running.iter().copied().any(|other| {
                                other != v && self.jobs[other].tag.as_deref() == Some(t)
                            })
                        });
                        slot && tag_free
                    })
                    .collect();

                victims.sort_by(|&a, &b| {
                    self.jobs[a]
                        .priority
                        .cmp(&self.jobs[b].priority)
                        .then_with(|| self.runtime[b].started_at.cmp(&self.runtime[a].started_at))
                        .then_with(|| self.jobs[b].id.cmp(&self.jobs[a].id))
                });

                if let Some(&victim) = victims.first() {
                    selected = Some((candidate, Some(victim)));
                    break;
                }
            }

            let Some((candidate, victim)) = selected else {
                break;
            };

            if let Some(victim) = victim {
                let finish_at = match self.runtime[victim].state {
                    State::Running { finish_at, .. } => finish_at,
                    _ => time,
                };
                let attempt = match self.runtime[victim].state {
                    State::Running { attempt, .. } => attempt,
                    _ => 0,
                };
                let remaining = finish_at.saturating_sub(time);
                events.push(format!(
                    "{} preempt {} by {}",
                    time, self.jobs[victim].id, self.jobs[candidate].id
                ));
                self.runtime[victim].state = State::Waiting {
                    ready_at: time,
                    attempt,
                };
                self.runtime[victim].age_origin = time;
                self.runtime[victim].remaining = Some(remaining);
            }

            let (attempt, remaining) = match self.runtime[candidate].state {
                State::Waiting { attempt, .. } => {
                    if let Some(rem) = self.runtime[candidate].remaining.take() {
                        (attempt, rem)
                    } else {
                        let next_attempt = attempt.saturating_add(1);
                        (next_attempt, self.jobs[candidate].duration)
                    }
                }
                _ => continue,
            };
            let finish_at = time.saturating_add(remaining);
            events.push(format!(
                "{} start {} {} {}",
                time, self.jobs[candidate].id, attempt, remaining
            ));
            self.runtime[candidate].state = State::Running { attempt, finish_at };
            self.runtime[candidate].started_at = time;
        }
    }

    fn effective_priority(&self, index: usize, time: u64) -> u8 {
        let base = self.jobs[index].priority;
        if self.aging_ms == 0 {
            return base;
        }
        let elapsed = time.saturating_sub(self.runtime[index].age_origin);
        let increase = (elapsed / self.aging_ms).min(9) as u8;
        base.saturating_add(increase).min(9)
    }
}

fn valid_token(value: &str, min_len: usize, max_len: usize) -> bool {
    let bytes = value.as_bytes();
    (min_len..=max_len).contains(&bytes.len())
        && bytes.iter().all(|b| {
            b.is_ascii_lowercase() || b.is_ascii_digit() || *b == b'-'
        })
}