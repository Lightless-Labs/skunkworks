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
struct Entry {
    job: Job,
    dependencies: Vec<usize>,
    state: State,
    remaining: Option<u64>,
    segment_order: u64,
}

pub struct Pool {
    entries: Vec<Entry>,
    by_id: std::collections::HashMap<String, usize>,
    capacity: usize,
    aging_ms: u64,
    retry_base: u64,
    retry_cap: u64,
    horizon: u64,
    started: bool,
    next_segment_order: u64,
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
            if job.tag.as_ref().is_some_and(|tag| !valid_token(tag, 1, 8)) {
                return Err(Error::BadTag(i));
            }
            if by_id.contains_key(&job.id) {
                return Err(Error::Duplicate(i));
            }
            by_id.insert(job.id.clone(), i);
        }

        let mut dependencies = Vec::with_capacity(jobs.len());
        for (i, job) in jobs.iter().enumerate() {
            let mut seen = std::collections::HashSet::new();
            let mut resolved = Vec::with_capacity(job.dependencies.len());
            for (j, name) in job.dependencies.iter().enumerate() {
                if !seen.insert(name.as_str()) {
                    return Err(Error::DuplicateDependency(i, j));
                }
                match by_id.get(name) {
                    Some(&index) => resolved.push(index),
                    None => return Err(Error::MissingDependency(i, j)),
                }
            }
            dependencies.push(resolved);
        }

        let mut color = vec![0u8; jobs.len()];
        fn visit(index: usize, deps: &[Vec<usize>], color: &mut [u8]) -> bool {
            if color[index] == 1 {
                return false;
            }
            if color[index] == 2 {
                return true;
            }
            color[index] = 1;
            for &dep in &deps[index] {
                if !visit(dep, deps, color) {
                    return false;
                }
            }
            color[index] = 2;
            true
        }
        for i in 0..jobs.len() {
            if !visit(i, &dependencies, &mut color) {
                return Err(Error::Cycle);
            }
        }

        let entries = jobs
            .into_iter()
            .enumerate()
            .map(|(i, job)| Entry {
                state: State::Waiting {
                    ready_at: job.release_at,
                    attempt: 0,
                },
                job,
                dependencies: dependencies[i].clone(),
                remaining: None,
                segment_order: 0,
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
            started: false,
            next_segment_order: 0,
        })
    }

    pub fn run(&mut self, until: u64) -> Result<Vec<String>, Error> {
        if until < self.horizon {
            return Err(Error::TimeReversed);
        }
        if until > 1_000_000_000 {
            return Err(Error::TimeRange);
        }

        let mut lines = Vec::new();
        let mut current = if self.started {
            self.horizon
        } else {
            0
        };
        let first_phase = !self.started;
        let mut process_phase = first_phase;

        loop {
            if process_phase {
                self.process_timestamp(current, &mut lines);
            }
            process_phase = true;

            if current >= until {
                break;
            }

            let mut next = None;
            for entry in &self.entries {
                match entry.state {
                    State::Waiting { ready_at, .. } if ready_at > current => {
                        next = Some(next.map_or(ready_at, |n: u64| n.min(ready_at)));
                    }
                    State::Running { finish_at, .. } if finish_at > current => {
                        next = Some(next.map_or(finish_at, |n: u64| n.min(finish_at)));
                    }
                    _ => {}
                }
            }
            let Some(next_time) = next else {
                break;
            };
            if next_time > until {
                break;
            }
            current = next_time;
        }

        self.horizon = until;
        self.started = true;
        Ok(lines)
    }

    pub fn status(&self, id: &str) -> Result<State, Error> {
        let index = *self.by_id.get(id).ok_or(Error::Unknown)?;
        Ok(self.entries[index].state.clone())
    }

    fn process_timestamp(&mut self, time: u64, lines: &mut Vec<String>) {
        let mut completed: Vec<usize> = self
            .entries
            .iter()
            .enumerate()
            .filter_map(|(i, e)| match e.state {
                State::Running { finish_at, .. } if finish_at == time => Some(i),
                _ => None,
            })
            .collect();
        completed.sort_by(|&a, &b| self.entries[a].job.id.as_bytes().cmp(self.entries[b].job.id.as_bytes()));

        for index in completed {
            let attempt = match self.entries[index].state {
                State::Running { attempt, .. } => attempt,
                _ => continue,
            };
            self.entries[index].remaining = None;
            let ok = attempt > self.entries[index].job.failures;
            if ok {
                self.entries[index].state = State::Succeeded;
                lines.push(format!(
                    "{} finish {} {} ok",
                    time, self.entries[index].job.id, attempt
                ));
            } else {
                lines.push(format!(
                    "{} finish {} {} fail",
                    time, self.entries[index].job.id, attempt
                ));
                if attempt >= self.entries[index].job.max_attempts {
                    self.entries[index].state = State::Failed;
                } else {
                    let delay = self.retry_delay(attempt);
                    let ready_at = time.saturating_add(delay);
                    self.entries[index].state = State::Waiting { ready_at, attempt };
                    lines.push(format!(
                        "{} retry {} {} {}",
                        time, self.entries[index].job.id, attempt, ready_at
                    ));
                }
            }
        }

        loop {
            let newly_blocked: Vec<usize> = self
                .entries
                .iter()
                .enumerate()
                .filter_map(|(i, e)| {
                    if !matches!(e.state, State::Waiting { .. }) {
                        return None;
                    }
                    e.dependencies
                        .iter()
                        .any(|&d| matches!(self.entries[d].state, State::Failed | State::Blocked))
                        .then_some(i)
                })
                .collect();
            if newly_blocked.is_empty() {
                break;
            }
            for &i in &newly_blocked {
                self.entries[i].state = State::Blocked;
            }
        }

        let mut blocked: Vec<usize> = self
            .entries
            .iter()
            .enumerate()
            .filter_map(|(i, e)| {
                (matches!(e.state, State::Blocked)
                    && e.dependencies
                        .iter()
                        .any(|&d| matches!(self.entries[d].state, State::Failed | State::Blocked)))
                .then_some(i)
            })
            .collect();
        blocked.sort_by(|&a, &b| self.entries[a].job.id.as_bytes().cmp(self.entries[b].job.id.as_bytes()));
        for &i in &blocked {
            let dep = self.entries[i]
                .dependencies
                .iter()
                .filter(|&&d| matches!(self.entries[d].state, State::Failed | State::Blocked))
                .min_by(|&&a, &&b| {
                    self.entries[a].job.id.as_bytes().cmp(self.entries[b].job.id.as_bytes())
                })
                .map(|&d| self.entries[d].job.id.clone())
                .unwrap_or_default();
            if !dep.is_empty() {
                lines.push(format!("{} skip {} {}", time, self.entries[i].job.id, dep));
            }
        }

        loop {
            let mut candidates: Vec<usize> = self
                .entries
                .iter()
                .enumerate()
                .filter_map(|(i, e)| {
                    let State::Waiting { ready_at, .. } = e.state else {
                        return None;
                    };
                    if ready_at > time
                        || !e.dependencies.iter().all(|&d| matches!(self.entries[d].state, State::Succeeded))
                    {
                        return None;
                    }
                    Some(i)
                })
                .collect();

            candidates.sort_by(|&a, &b| {
                let pa = self.effective_priority(a, time);
                let pb = self.effective_priority(b, time);
                pb.cmp(&pa).then(a.cmp(&b))
            });

            let mut selected = None;
            for candidate in candidates {
                if let Some(victim) = self.find_victim(candidate) {
                    selected = Some((candidate, Some(victim)));
                    break;
                }
                if self.can_start(candidate) {
                    selected = Some((candidate, None));
                    break;
                }
            }
            let Some((candidate, victim)) = selected else {
                break;
            };

            if let Some(victim_index) = victim {
                let remaining = match self.entries[victim_index].state {
                    State::Running { finish_at, .. } => finish_at - time,
                    _ => continue,
                };
                lines.push(format!(
                    "{} preempt {} by {}",
                    time, self.entries[victim_index].job.id, self.entries[candidate].job.id
                ));
                let attempt = match self.entries[victim_index].state {
                    State::Running { attempt, .. } => attempt,
                    _ => 0,
                };
                self.entries[victim_index].state = State::Waiting {
                    ready_at: time,
                    attempt,
                };
                self.entries[victim_index].remaining = Some(remaining);
            }

            let (attempt, duration) = match self.entries[candidate].state {
                State::Waiting { attempt, .. } => {
                    if let Some(remaining) = self.entries[candidate].remaining.take() {
                        (attempt, remaining)
                    } else {
                        (attempt.saturating_add(1), self.entries[candidate].job.duration)
                    }
                }
                _ => continue,
            };
            let finish_at = time.saturating_add(duration);
            self.next_segment_order = self.next_segment_order.saturating_add(1);
            self.entries[candidate].segment_order = self.next_segment_order;
            self.entries[candidate].state = State::Running { attempt, finish_at };
            lines.push(format!(
                "{} start {} {} {}",
                time, self.entries[candidate].job.id, attempt, duration
            ));
        }
    }

    fn can_start(&self, candidate: usize) -> bool {
        let running_count = self
            .entries
            .iter()
            .filter(|e| matches!(e.state, State::Running { .. }))
            .count();
        if running_count >= self.capacity {
            return false;
        }
        !self.tag_is_occupied(candidate, None)
    }

    fn find_victim(&self, candidate: usize) -> Option<usize> {
        let candidate_priority = self.entries[candidate].job.priority;
        let candidate_tag = self.entries[candidate].job.tag.as_deref();
        let running_count = self
            .entries
            .iter()
            .filter(|e| matches!(e.state, State::Running { .. }))
            .count();

        let mut victims: Vec<usize> = self
            .entries
            .iter()
            .enumerate()
            .filter_map(|(i, entry)| {
                let State::Running { .. } = entry.state else {
                    return None;
                };
                if candidate_priority < entry.job.priority.saturating_add(2) {
                    return None;
                }
                let remaining_running = running_count - 1;
                if remaining_running >= self.capacity {
                    return None;
                }
                if let Some(tag) = candidate_tag {
                    let held_by_other = self.entries.iter().enumerate().any(|(j, other)| {
                        j != i
                            && matches!(other.state, State::Running { .. })
                            && other.job.tag.as_deref() == Some(tag)
                    });
                    if held_by_other {
                        return None;
                    }
                }
                Some(i)
            })
            .collect();

        victims.sort_by(|&a, &b| {
            self.entries[a]
                .job
                .priority
                .cmp(&self.entries[b].job.priority)
                .then_with(|| self.entries[b].segment_order.cmp(&self.entries[a].segment_order))
                .then_with(|| self.entries[b].job.id.as_bytes().cmp(self.entries[a].job.id.as_bytes()))
        });
        victims.into_iter().next()
    }

    fn tag_is_occupied(&self, candidate: usize, excluded: Option<usize>) -> bool {
        let Some(tag) = self.entries[candidate].job.tag.as_deref() else {
            return false;
        };
        self.entries.iter().enumerate().any(|(i, entry)| {
            Some(i) != excluded
                && matches!(entry.state, State::Running { .. })
                && entry.job.tag.as_deref() == Some(tag)
        })
    }

    fn effective_priority(&self, index: usize, time: u64) -> u8 {
        let base = self.entries[index].job.priority;
        if self.aging_ms == 0 {
            return base;
        }
        let ready_at = match self.entries[index].state {
            State::Waiting { ready_at, .. } => ready_at,
            _ => time,
        };
        let age = time.saturating_sub(ready_at) / self.aging_ms;
        (base as u64 + age).min(9) as u8
    }

    fn retry_delay(&self, attempt: u8) -> u64 {
        let mut delay = self.retry_base;
        for _ in 1..attempt {
            delay = delay.saturating_mul(2).min(self.retry_cap);
        }
        delay.min(self.retry_cap)
    }
}

fn valid_token(value: &str, min_len: usize, max_len: usize) -> bool {
    let bytes = value.as_bytes();
    (min_len..=max_len).contains(&bytes.len())
        && bytes
            .iter()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || *b == b'-')
}