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

struct Entry {
    job: Job,
    state: State,
    input_order: usize,
    remaining: Option<u64>,
    segment_started: u64,
}

pub struct Pool {
    entries: Vec<Entry>,
    by_id: std::collections::HashMap<String, usize>,
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
            || retry_base == 0
            || retry_base > retry_cap
            || retry_cap > 1_000_000_000
        {
            return Err(Error::Config);
        }

        let mut by_id = std::collections::HashMap::new();
        for (i, job) in jobs.iter().enumerate() {
            if !valid_id(&job.id) {
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
            if job.tag.as_ref().is_some_and(|s| !valid_tag(s)) {
                return Err(Error::BadTag(i));
            }
            if by_id.contains_key(&job.id) {
                return Err(Error::Duplicate(i));
            }
            by_id.insert(job.id.clone(), i);
        }

        for (i, job) in jobs.iter().enumerate() {
            let mut seen = std::collections::HashSet::new();
            for (k, dep) in job.dependencies.iter().enumerate() {
                if !seen.insert(dep) {
                    return Err(Error::DuplicateDependency(i, k));
                }
                if !by_id.contains_key(dep) {
                    return Err(Error::MissingDependency(i, k));
                }
            }
        }

        let mut indegree = vec![0usize; jobs.len()];
        let mut outgoing = vec![Vec::new(); jobs.len()];
        for (i, job) in jobs.iter().enumerate() {
            indegree[i] = job.dependencies.len();
            for dep in &job.dependencies {
                outgoing[by_id[dep]].push(i);
            }
        }
        let mut queue: std::collections::VecDeque<usize> = indegree
            .iter()
            .enumerate()
            .filter_map(|(i, &d)| (d == 0).then_some(i))
            .collect();
        let mut visited = 0;
        while let Some(i) = queue.pop_front() {
            visited += 1;
            for &next in &outgoing[i] {
                indegree[next] -= 1;
                if indegree[next] == 0 {
                    queue.push_back(next);
                }
            }
        }
        if visited != jobs.len() {
            return Err(Error::Cycle);
        }

        let entries = jobs
            .into_iter()
            .enumerate()
            .map(|(input_order, job)| Entry {
                state: State::Waiting {
                    ready_at: job.release_at,
                    attempt: 0,
                },
                job,
                input_order,
                remaining: None,
                segment_started: 0,
            })
            .collect();

        Ok(Self {
            entries,
            by_id,
            capacity,
            aging_ms,
            retry_base,
            retry_cap,
            horizon: 0,
            has_run: false,
        })
    }

    pub fn status(&self, id: &str) -> Result<State, Error> {
        let &i = self.by_id.get(id).ok_or(Error::Unknown)?;
        Ok(self.entries[i].state.clone())
    }

    pub fn run(&mut self, until: u64) -> Result<Vec<String>, Error> {
        if until < self.horizon {
            return Err(Error::TimeReversed);
        }
        if until > 1_000_000_000 {
            return Err(Error::TimeRange);
        }
        if self.has_run && until == self.horizon {
            return Ok(Vec::new());
        }

        let mut lines = Vec::new();
        let mut now = if self.has_run {
            self.next_event_after(self.horizon, until)
        } else {
            Some(0)
        };

        while let Some(t) = now {
            if t > until {
                break;
            }
            self.process_time(t, &mut lines);
            now = self.next_event_after(t, until);
        }

        self.horizon = until;
        self.has_run = true;
        Ok(lines)
    }

    fn next_event_after(&self, after: u64, until: u64) -> Option<u64> {
        let mut next = None;
        let mut consider = |t: u64| {
            if t > after && t <= until {
                next = Some(next.map_or(t, |old: u64| old.min(t)));
            }
        };
        for entry in &self.entries {
            match entry.state {
                State::Waiting { ready_at, .. } => consider(ready_at),
                State::Running { finish_at, .. } => consider(finish_at),
                _ => {}
            }
        }
        next
    }

    fn process_time(&mut self, t: u64, lines: &mut Vec<String>) {
        let mut completed: Vec<usize> = self
            .entries
            .iter()
            .enumerate()
            .filter_map(|(i, e)| match e.state {
                State::Running { finish_at, .. } if finish_at == t => Some(i),
                _ => None,
            })
            .collect();
        completed.sort_by(|&a, &b| self.entries[a].job.id.cmp(&self.entries[b].job.id));

        for i in completed {
            let attempt = match self.entries[i].state {
                State::Running { attempt, .. } => attempt,
                _ => continue,
            };
            if attempt > self.entries[i].job.failures {
                self.entries[i].state = State::Succeeded;
                lines.push(format!(
                    "{t} finish {} {attempt} ok",
                    self.entries[i].job.id
                ));
            } else {
                lines.push(format!(
                    "{t} finish {} {attempt} fail",
                    self.entries[i].job.id
                ));
                if attempt >= self.entries[i].job.max_attempts {
                    self.entries[i].state = State::Failed;
                } else {
                    let shift = (attempt - 1) as u32;
                    let delay = self
                        .retry_base
                        .checked_shl(shift)
                        .unwrap_or(u64::MAX)
                        .min(self.retry_cap);
                    let ready = t.saturating_add(delay);
                    self.entries[i].state = State::Waiting {
                        ready_at: ready,
                        attempt,
                    };
                    self.entries[i].remaining = None;
                    lines.push(format!(
                        "{t} retry {} {attempt} {ready}",
                        self.entries[i].job.id
                    ));
                }
            }
        }

        let mut newly_blocked = Vec::new();
        loop {
            let mut changed = false;
            for i in 0..self.entries.len() {
                if !matches!(self.entries[i].state, State::Waiting { .. }) {
                    continue;
                }
                let blocked = self.entries[i].job.dependencies.iter().any(|dep| {
                    let di = self.by_id[dep];
                    matches!(self.entries[di].state, State::Failed | State::Blocked)
                });
                if blocked {
                    self.entries[i].state = State::Blocked;
                    newly_blocked.push(i);
                    changed = true;
                }
            }
            if !changed {
                break;
            }
        }
        newly_blocked.sort_by(|&a, &b| self.entries[a].job.id.cmp(&self.entries[b].job.id));
        for i in newly_blocked {
            lines.push(format!("{t} skip {} dependency", self.entries[i].job.id));
        }

        self.schedule(t, lines);
    }

    fn schedule(&mut self, t: u64, lines: &mut Vec<String>) {
        loop {
            let mut candidates: Vec<usize> = self
                .entries
                .iter()
                .enumerate()
                .filter_map(|(i, e)| match e.state {
                    State::Waiting { ready_at, .. }
                        if ready_at <= t
                            && e.job.dependencies.iter().all(|dep| {
                                matches!(
                                    self.entries[self.by_id[dep]].state,
                                    State::Succeeded
                                )
                            }) =>
                    {
                        Some(i)
                    }
                    _ => None,
                })
                .collect();

            candidates.sort_by(|&a, &b| {
                let pa = self.effective_priority(a, t);
                let pb = self.effective_priority(b, t);
                pb.cmp(&pa)
                    .then_with(|| self.entries[a].input_order.cmp(&self.entries[b].input_order))
            });

            let mut made_start = false;
            for candidate in candidates {
                if self.can_start_direct(candidate) {
                    self.start(candidate, t, lines);
                    made_start = true;
                    break;
                }

                let victims = self.possible_victims(candidate);
                if let Some(victim) = victims.first().copied() {
                    let victim_id = self.entries[victim].job.id.clone();
                    let candidate_id = self.entries[candidate].job.id.clone();
                    let finish = match self.entries[victim].state {
                        State::Running { finish_at, .. } => finish_at,
                        _ => continue,
                    };
                    let remaining = finish - t;
                    let attempt = match self.entries[victim].state {
                        State::Running { attempt, .. } => attempt,
                        _ => continue,
                    };
                    self.entries[victim].state = State::Waiting {
                        ready_at: t,
                        attempt,
                    };
                    self.entries[victim].remaining = Some(remaining);
                    lines.push(format!("{t} preempt {victim_id} by {candidate_id}"));
                    self.start(candidate, t, lines);
                    made_start = true;
                    break;
                }
            }
            if !made_start {
                break;
            }
        }
    }

    fn effective_priority(&self, i: usize, t: u64) -> u8 {
        let ready_at = match self.entries[i].state {
            State::Waiting { ready_at, .. } => ready_at,
            _ => t,
        };
        let base = self.entries[i].job.priority;
        if self.aging_ms == 0 {
            base
        } else {
            let age = (t - ready_at) / self.aging_ms;
            (base as u64 + age).min(9) as u8
        }
    }

    fn can_start_direct(&self, candidate: usize) -> bool {
        let running = self
            .entries
            .iter()
            .filter(|e| matches!(e.state, State::Running { .. }))
            .count();
        if running >= self.capacity {
            return false;
        }
        let tag = self.entries[candidate].job.tag.as_ref();
        !self.entries.iter().any(|e| {
            matches!(e.state, State::Running { .. })
                && tag.is_some()
                && e.job.tag.as_ref() == tag
        })
    }

    fn possible_victims(&self, candidate: usize) -> Vec<usize> {
        let candidate_tag = self.entries[candidate].job.tag.as_ref();
        let running: Vec<usize> = self
            .entries
            .iter()
            .enumerate()
            .filter_map(|(i, e)| matches!(e.state, State::Running { .. }).then_some(i))
            .collect();
        let at_capacity = running.len() >= self.capacity;
        let mut victims: Vec<usize> = running
            .into_iter()
            .filter(|&i| {
                (candidate_tag.is_some()
                    && self.entries[i].job.tag.as_ref() == candidate_tag)
                    || at_capacity
            })
            .filter(|&i| {
                self.entries[candidate].job.priority
                    >= self.entries[i].job.priority.saturating_add(2)
            })
            .filter(|&i| {
                let remaining_running = self
                    .entries
                    .iter()
                    .filter(|e| matches!(e.state, State::Running { .. }))
                    .count()
                    - 1;
                let slot_free = remaining_running < self.capacity;
                let tag_free = candidate_tag.is_none()
                    || !self.entries.iter().enumerate().any(|(j, e)| {
                        j != i
                            && matches!(e.state, State::Running { .. })
                            && e.job.tag.as_ref() == candidate_tag
                    });
                slot_free && tag_free
            })
            .collect();

        victims.sort_by(|&a, &b| {
            self.entries[a]
                .job
                .priority
                .cmp(&self.entries[b].job.priority)
                .then_with(|| {
                    let sa = match self.entries[a].state {
                        State::Running { .. } => self.entries[a].segment_started,
                        _ => 0,
                    };
                    let sb = match self.entries[b].state {
                        State::Running { .. } => self.entries[b].segment_started,
                        _ => 0,
                    };
                    sb.cmp(&sa)
                })
                .then_with(|| self.entries[b].job.id.cmp(&self.entries[a].job.id))
        });
        victims
    }

    fn start(&mut self, i: usize, t: u64, lines: &mut Vec<String>) {
        let (attempt, remaining) = match self.entries[i].state {
            State::Waiting { attempt, .. } => {
                if let Some(remaining) = self.entries[i].remaining.take() {
                    (attempt, remaining)
                } else {
                    let next = attempt.saturating_add(1);
                    (next, self.entries[i].job.duration)
                }
            }
            _ => return,
        };
        let finish_at = t.saturating_add(remaining);
        self.entries[i].state = State::Running { attempt, finish_at };
        self.entries[i].segment_started = t;
        lines.push(format!(
            "{t} start {} {attempt} {remaining}",
            self.entries[i].job.id
        ));
    }
}

fn valid_id(s: &str) -> bool {
    !s.is_empty()
        && s.len() <= 16
        && s.bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
}

fn valid_tag(s: &str) -> bool {
    !s.is_empty()
        && s.len() <= 8
        && s.bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
}