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
    state: State,
    age_anchor: u64,
    remaining: Option<u64>,
    segment_start: u64,
}

pub struct Pool {
    jobs: Vec<RuntimeJob>,
    index: std::collections::HashMap<String, usize>,
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
            || retry_base == 0
            || retry_base > retry_cap
            || retry_cap > 1_000_000_000
        {
            return Err(Error::Config);
        }

        let mut index = std::collections::HashMap::new();
        for (i, job) in jobs.iter().enumerate() {
            if !Self::valid_token(&job.id, 1, 16) {
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
                if !Self::valid_token(tag, 1, 8) {
                    return Err(Error::BadTag(i));
                }
            }
            if index.contains_key(&job.id) {
                return Err(Error::Duplicate(i));
            }
            index.insert(job.id.clone(), i);
        }

        let mut deps = vec![Vec::new(); jobs.len()];
        for (i, job) in jobs.iter().enumerate() {
            let mut seen = std::collections::HashSet::new();
            for (j, dep) in job.dependencies.iter().enumerate() {
                if !seen.insert(dep.as_str()) {
                    return Err(Error::DuplicateDependency(i, j));
                }
                match index.get(dep) {
                    Some(&d) => deps[i].push(d),
                    None => return Err(Error::MissingDependency(i, j)),
                }
            }
        }

        // Kahn's algorithm detects cycles without recursion, so dependency
        // chains of arbitrary depth cannot overflow the call stack.
        let mut remaining_deps: Vec<usize> = deps.iter().map(Vec::len).collect();
        let mut dependents = vec![Vec::new(); jobs.len()];
        for (job_index, job_deps) in deps.iter().enumerate() {
            for &dep_index in job_deps {
                dependents[dep_index].push(job_index);
            }
        }
        let mut ready = std::collections::VecDeque::new();
        for (i, &count) in remaining_deps.iter().enumerate() {
            if count == 0 {
                ready.push_back(i);
            }
        }
        let mut visited = 0usize;
        while let Some(done) = ready.pop_front() {
            visited += 1;
            for &dependent in &dependents[done] {
                remaining_deps[dependent] -= 1;
                if remaining_deps[dependent] == 0 {
                    ready.push_back(dependent);
                }
            }
        }
        if visited != jobs.len() {
            return Err(Error::Cycle);
        }

        let runtime = jobs
            .into_iter()
            .map(|job| RuntimeJob {
                state: State::Waiting {
                    ready_at: job.release_at,
                    attempt: 0,
                },
                age_anchor: job.release_at,
                remaining: None,
                segment_start: 0,
                job,
            })
            .collect();

        Ok(Self {
            jobs: runtime,
            index,
            capacity,
            aging_ms,
            retry_base,
            retry_cap,
            horizon: 0,
            first_run: true,
        })
    }

    fn valid_token(s: &str, min: usize, max: usize) -> bool {
        let b = s.as_bytes();
        (min..=max).contains(&b.len())
            && b.iter().all(|c| {
                c.is_ascii_lowercase() || c.is_ascii_digit() || *c == b'-'
            })
    }

    pub fn status(&self, id: &str) -> Result<State, Error> {
        self.index
            .get(id)
            .map(|&i| self.jobs[i].state.clone())
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
        let mut time = if self.first_run { 0 } else { self.horizon };
        let mut process_current = self.first_run;

        loop {
            if process_current {
                self.process_time(time, &mut lines);
            }

            let next = self.next_event_after(time);
            match next {
                Some(t) if t <= until => {
                    time = t;
                    process_current = true;
                }
                _ => break,
            }
        }

        self.horizon = until;
        self.first_run = false;
        Ok(lines)
    }

    fn process_time(&mut self, time: u64, lines: &mut Vec<String>) {
        let mut completing: Vec<usize> = self
            .jobs
            .iter()
            .enumerate()
            .filter_map(|(i, r)| match r.state {
                State::Running { finish_at, .. } if finish_at == time => Some(i),
                _ => None,
            })
            .collect();
        completing.sort_by(|&a, &b| self.jobs[a].job.id.cmp(&self.jobs[b].job.id));

        for i in completing {
            let attempt = match self.jobs[i].state {
                State::Running { attempt, .. } => attempt,
                _ => continue,
            };
            let ok = attempt > self.jobs[i].job.failures;
            let id = self.jobs[i].job.id.clone();
            if ok {
                self.jobs[i].state = State::Succeeded;
                self.jobs[i].remaining = None;
                lines.push(format!("{time} finish {id} {attempt} ok"));
            } else {
                lines.push(format!("{time} finish {id} {attempt} fail"));
                if attempt >= self.jobs[i].job.max_attempts {
                    self.jobs[i].state = State::Failed;
                    self.jobs[i].remaining = None;
                } else {
                    let shift = u32::from(attempt.saturating_sub(1));
                    let delay = self
                        .retry_base
                        .checked_shl(shift)
                        .unwrap_or(u64::MAX)
                        .min(self.retry_cap);
                    let ready = time.saturating_add(delay);
                    self.jobs[i].state = State::Waiting {
                        ready_at: ready,
                        attempt,
                    };
                    self.jobs[i].age_anchor = ready;
                    self.jobs[i].remaining = None;
                    lines.push(format!("{time} retry {id} {attempt} {ready}"));
                }
            }
        }

        self.propagate_failures(time, lines);
        self.schedule(time, lines);
    }

    fn propagate_failures(&mut self, time: u64, lines: &mut Vec<String>) {
        let mut newly_blocked = Vec::new();

        loop {
            let mut wave = Vec::new();
            for i in 0..self.jobs.len() {
                if !matches!(self.jobs[i].state, State::Waiting { .. }) {
                    continue;
                }
                let has_failed_dependency = self.jobs[i].job.dependencies.iter().any(|dep| {
                    let d = self.index[dep];
                    matches!(self.jobs[d].state, State::Failed | State::Blocked)
                });
                if has_failed_dependency {
                    wave.push(i);
                }
            }

            if wave.is_empty() {
                break;
            }

            for i in wave {
                self.jobs[i].state = State::Blocked;
                newly_blocked.push(i);
            }
        }

        newly_blocked.sort_by(|&a, &b| self.jobs[a].job.id.cmp(&self.jobs[b].job.id));
        for i in newly_blocked {
            let id = self.jobs[i].job.id.clone();
            lines.push(format!("{time} skip {id} dependency"));
        }
    }

    fn schedule(&mut self, time: u64, lines: &mut Vec<String>) {
        loop {
            let mut candidates: Vec<usize> = self
                .jobs
                .iter()
                .enumerate()
                .filter_map(|(i, r)| match r.state {
                    State::Waiting { ready_at, .. }
                        if ready_at <= time && self.dependencies_succeeded(i) =>
                    {
                        Some(i)
                    }
                    _ => None,
                })
                .collect();

            candidates.sort_by(|&a, &b| {
                let pa = self.effective_priority(a, time);
                let pb = self.effective_priority(b, time);
                pb.cmp(&pa).then_with(|| a.cmp(&b))
            });

            let mut did_start = false;
            for candidate in candidates {
                if self.can_start_without_preemption(candidate) {
                    self.start(candidate, time, lines);
                    did_start = true;
                    break;
                }

                if let Some(victim) = self.choose_victim(candidate) {
                    let victim_id = self.jobs[victim].job.id.clone();
                    let candidate_id = self.jobs[candidate].job.id.clone();
                    let remaining = match self.jobs[victim].state {
                        State::Running { finish_at, .. } => finish_at - time,
                        _ => 0,
                    };
                    let attempt = match self.jobs[victim].state {
                        State::Running { attempt, .. } => attempt,
                        _ => 0,
                    };
                    self.jobs[victim].state = State::Waiting {
                        ready_at: time,
                        attempt,
                    };
                    self.jobs[victim].age_anchor = time;
                    self.jobs[victim].remaining = Some(remaining);
                    lines.push(format!("{time} preempt {victim_id} by {candidate_id}"));
                    self.start(candidate, time, lines);
                    did_start = true;
                    break;
                }
            }

            if !did_start {
                break;
            }
        }
    }

    fn dependencies_succeeded(&self, i: usize) -> bool {
        self.jobs[i].job.dependencies.iter().all(|dep| {
            matches!(self.jobs[self.index[dep]].state, State::Succeeded)
        })
    }

    fn effective_priority(&self, i: usize, time: u64) -> u8 {
        let base = self.jobs[i].job.priority;
        if self.aging_ms == 0 {
            return base;
        }
        let elapsed = time.saturating_sub(self.jobs[i].age_anchor);
        let gained = elapsed / self.aging_ms;
        (u64::from(base) + gained).min(9) as u8
    }

    fn can_start_without_preemption(&self, candidate: usize) -> bool {
        let running = self
            .jobs
            .iter()
            .filter(|r| matches!(r.state, State::Running { .. }))
            .count();
        if running >= self.capacity {
            return false;
        }
        let tag = self.jobs[candidate].job.tag.as_deref();
        if let Some(tag) = tag {
            !self.jobs.iter().any(|r| {
                matches!(r.state, State::Running { .. }) && r.job.tag.as_deref() == Some(tag)
            })
        } else {
            true
        }
    }

    fn choose_victim(&self, candidate: usize) -> Option<usize> {
        let candidate_tag = self.jobs[candidate].job.tag.as_deref();
        let running: Vec<usize> = self
            .jobs
            .iter()
            .enumerate()
            .filter_map(|(i, r)| matches!(r.state, State::Running { .. }).then_some(i))
            .collect();

        let slot_free_after = running.len().saturating_sub(1) < self.capacity;
        if !slot_free_after {
            return None;
        }

        let eligible: Vec<usize> = running
            .into_iter()
            .filter(|&v| {
                self.jobs[candidate].job.priority
                    >= self.jobs[v].job.priority.saturating_add(2)
            })
            .filter(|&v| {
                if let Some(tag) = candidate_tag {
                    !self.jobs.iter().enumerate().any(|(i, r)| {
                        i != v
                            && matches!(r.state, State::Running { .. })
                            && r.job.tag.as_deref() == Some(tag)
                    })
                } else {
                    true
                }
            })
            .collect();

        eligible.into_iter().min_by(|&a, &b| {
            self.jobs[a]
                .job
                .priority
                .cmp(&self.jobs[b].job.priority)
                .then_with(|| {
                    let sa = self.jobs[a].segment_start;
                    let sb = self.jobs[b].segment_start;
                    sb.cmp(&sa)
                })
                .then_with(|| self.jobs[b].job.id.cmp(&self.jobs[a].job.id))
        })
    }

    fn start(&mut self, i: usize, time: u64, lines: &mut Vec<String>) {
        let (attempt, remaining) = match self.jobs[i].state.clone() {
            State::Waiting { attempt, .. } => {
                if let Some(saved) = self.jobs[i].remaining.take() {
                    (attempt, saved)
                } else {
                    let next_attempt = attempt.saturating_add(1);
                    (next_attempt, self.jobs[i].job.duration)
                }
            }
            _ => return,
        };
        let finish = time.saturating_add(remaining);
        let id = self.jobs[i].job.id.clone();
        self.jobs[i].segment_start = time;
        self.jobs[i].state = State::Running {
            attempt,
            finish_at: finish,
        };
        lines.push(format!("{time} start {id} {attempt} {remaining}"));
    }

    fn next_event_after(&self, time: u64) -> Option<u64> {
        let mut next: Option<u64> = None;
        for r in &self.jobs {
            let candidate = match r.state {
                State::Waiting { ready_at, .. } if ready_at > time => Some(ready_at),
                State::Running { finish_at, .. } if finish_at > time => Some(finish_at),
                _ => None,
            };
            if let Some(t) = candidate {
                next = Some(next.map_or(t, |n| n.min(t)));
            }
        }
        next
    }
}