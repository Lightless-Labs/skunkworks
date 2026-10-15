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
    dependencies: Vec<usize>,
    state: State,
    remaining: Option<u64>,
    started_at: u64,
}

pub struct Pool {
    entries: Vec<Entry>,
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

        let mut seen = std::collections::HashSet::new();
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
            if !seen.insert(job.id.clone()) {
                return Err(Error::Duplicate(i));
            }
        }

        let id_to_index: std::collections::HashMap<String, usize> = jobs
            .iter()
            .enumerate()
            .map(|(i, job)| (job.id.clone(), i))
            .collect();

        let mut dependencies = Vec::with_capacity(jobs.len());
        for (i, job) in jobs.iter().enumerate() {
            let mut dep_seen = std::collections::HashSet::new();
            let mut resolved = Vec::with_capacity(job.dependencies.len());
            for (j, spelling) in job.dependencies.iter().enumerate() {
                if !dep_seen.insert(spelling.clone()) {
                    return Err(Error::DuplicateDependency(i, j));
                }
                match id_to_index.get(spelling) {
                    Some(&index) => resolved.push(index),
                    None => return Err(Error::MissingDependency(i, j)),
                }
            }
            dependencies.push(resolved);
        }

        let mut marks = vec![0u8; jobs.len()];
        fn visit(i: usize, dependencies: &[Vec<usize>], marks: &mut [u8]) -> bool {
            if marks[i] == 1 {
                return false;
            }
            if marks[i] == 2 {
                return true;
            }
            marks[i] = 1;
            for &dep in &dependencies[i] {
                if !visit(dep, dependencies, marks) {
                    return false;
                }
            }
            marks[i] = 2;
            true
        }
        for i in 0..jobs.len() {
            if !visit(i, &dependencies, &mut marks) {
                return Err(Error::Cycle);
            }
        }

        let entries = jobs
            .into_iter()
            .zip(dependencies)
            .map(|(job, dependencies)| Entry {
                state: State::Waiting {
                    ready_at: job.release_at,
                    attempt: 0,
                },
                job,
                dependencies,
                remaining: None,
                started_at: 0,
            })
            .collect();

        Ok(Self {
            entries,
            capacity,
            aging_ms,
            retry_base,
            retry_cap,
            horizon: 0,
            has_run: false,
        })
    }

    pub fn status(&self, id: &str) -> Result<State, Error> {
        self.entries
            .iter()
            .find(|entry| entry.job.id == id)
            .map(|entry| entry.state.clone())
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

        let mut trace = Vec::new();
        let mut time = if self.has_run {
            self.horizon.saturating_add(1)
        } else {
            0
        };

        while time <= until {
            let mut completed: Vec<usize> = self
                .entries
                .iter()
                .enumerate()
                .filter_map(|(i, entry)| match entry.state {
                    State::Running { finish_at, .. } if finish_at == time => Some(i),
                    _ => None,
                })
                .collect();
            completed.sort_by(|&a, &b| self.entries[a].job.id.cmp(&self.entries[b].job.id));

            for &i in &completed {
                let attempt = match self.entries[i].state {
                    State::Running { attempt, .. } => attempt,
                    _ => continue,
                };
                if attempt > self.entries[i].job.failures {
                    self.entries[i].state = State::Succeeded;
                    trace.push(format!("{} finish {} {} ok", time, self.entries[i].job.id, attempt));
                } else {
                    trace.push(format!("{} finish {} {} fail", time, self.entries[i].job.id, attempt));
                    if attempt >= self.entries[i].job.max_attempts {
                        self.entries[i].state = State::Failed;
                    } else {
                        let shift = u32::from(attempt.saturating_sub(1));
                        let delay = self
                            .retry_base
                            .checked_shl(shift)
                            .unwrap_or(u64::MAX)
                            .min(self.retry_cap);
                        let ready_at = time.saturating_add(delay);
                        self.entries[i].state = State::Waiting { ready_at, attempt };
                        self.entries[i].remaining = None;
                        trace.push(format!(
                            "{} retry {} {} {}",
                            time, self.entries[i].job.id, attempt, ready_at
                        ));
                    }
                }
            }

            loop {
                let mut newly_blocked = Vec::new();
                for i in 0..self.entries.len() {
                    if !matches!(self.entries[i].state, State::Waiting { .. }) {
                        continue;
                    }
                    if let Some(&dep) = self.entries[i].dependencies.iter().find(|&&dep| {
                        matches!(
                            self.entries[dep].state,
                            State::Failed | State::Blocked
                        )
                    }) {
                        self.entries[i].state = State::Blocked;
                        newly_blocked.push((i, dep));
                    }
                }
                if newly_blocked.is_empty() {
                    break;
                }
                newly_blocked.sort_by(|(a, _), (b, _)| {
                    self.entries[*a].job.id.cmp(&self.entries[*b].job.id)
                });
                for (i, dep) in newly_blocked {
                    trace.push(format!(
                        "{} skip {} {}",
                        time, self.entries[i].job.id, self.entries[dep].job.id
                    ));
                }
            }

            loop {
                let running: Vec<usize> = self
                    .entries
                    .iter()
                    .enumerate()
                    .filter_map(|(i, entry)| {
                        if matches!(entry.state, State::Running { .. }) {
                            Some(i)
                        } else {
                            None
                        }
                    })
                    .collect();
                let occupied_tags: std::collections::HashSet<String> = running
                    .iter()
                    .filter_map(|&i| self.entries[i].job.tag.clone())
                    .collect();

                let mut candidates: Vec<usize> = self
                    .entries
                    .iter()
                    .enumerate()
                    .filter_map(|(i, entry)| {
                        let ready_at = match entry.state {
                            State::Waiting { ready_at, .. } => ready_at,
                            _ => return None,
                        };
                        if ready_at > time
                            || !entry
                                .dependencies
                                .iter()
                                .all(|&d| matches!(self.entries[d].state, State::Succeeded))
                        {
                            return None;
                        }
                        Some(i)
                    })
                    .collect();

                candidates.sort_by(|&a, &b| {
                    let pa = self.effective_priority(a, time);
                    let pb = self.effective_priority(b, time);
                    pb.cmp(&pa).then_with(|| a.cmp(&b))
                });

                let mut action = None;
                for candidate in candidates {
                    let candidate_tag = self.entries[candidate].job.tag.as_ref();
                    let tag_free = candidate_tag.map_or(true, |tag| !occupied_tags.contains(tag));
                    if running.len() < self.capacity && tag_free {
                        action = Some((candidate, None));
                        break;
                    }

                    let mut victims: Vec<usize> = running
                        .iter()
                        .copied()
                        .filter(|&victim| {
                            self.entries[candidate].job.priority
                                >= self.entries[victim].job.priority.saturating_add(2)
                        })
                        .filter(|&victim| {
                            let slot_free_after = running.len() - 1 < self.capacity;
                            let tag_free_after = candidate_tag.map_or(true, |tag| {
                                !occupied_tags.contains(tag)
                                    || self.entries[victim].job.tag.as_deref() == Some(tag)
                            });
                            slot_free_after && tag_free_after
                        })
                        .collect();

                    victims.sort_by(|&a, &b| {
                        self.entries[a]
                            .job
                            .priority
                            .cmp(&self.entries[b].job.priority)
                            .then_with(|| self.entries[b].started_at.cmp(&self.entries[a].started_at))
                            .then_with(|| self.entries[b].job.id.cmp(&self.entries[a].job.id))
                    });

                    if let Some(&victim) = victims.first() {
                        action = Some((candidate, Some(victim)));
                        break;
                    }
                }

                let Some((candidate, victim)) = action else {
                    break;
                };

                if let Some(victim) = victim {
                    let (attempt, finish_at) = match self.entries[victim].state {
                        State::Running { attempt, finish_at } => (attempt, finish_at),
                        _ => continue,
                    };
                    let remaining = finish_at.saturating_sub(time);
                    self.entries[victim].state = State::Waiting {
                        ready_at: time,
                        attempt,
                    };
                    self.entries[victim].remaining = Some(remaining);
                    trace.push(format!(
                        "{} preempt {} by {}",
                        time, self.entries[victim].job.id, self.entries[candidate].job.id
                    ));
                }

                let (attempt, remaining) = match self.entries[candidate].state {
                    State::Waiting { attempt, .. } => match self.entries[candidate].remaining {
                        Some(saved) => (attempt, saved),
                        None => (attempt.saturating_add(1), self.entries[candidate].job.duration),
                    },
                    _ => continue,
                };
                self.entries[candidate].remaining = None;
                self.entries[candidate].started_at = time;
                let finish_at = time.saturating_add(remaining);
                self.entries[candidate].state = State::Running { attempt, finish_at };
                trace.push(format!(
                    "{} start {} {} {}",
                    time, self.entries[candidate].job.id, attempt, remaining
                ));
            }

            let next = self.entries.iter().filter_map(|entry| match entry.state {
                State::Waiting { ready_at, .. } if ready_at > time => Some(ready_at),
                State::Running { finish_at, .. } if finish_at > time => Some(finish_at),
                _ => None,
            }).min();

            match next {
                Some(next_time) if next_time <= until => time = next_time,
                _ => break,
            }
        }

        self.horizon = until;
        self.has_run = true;
        Ok(trace)
    }
}

impl Pool {
    fn effective_priority(&self, index: usize, time: u64) -> u8 {
        let base = self.entries[index].job.priority;
        if self.aging_ms == 0 {
            return base;
        }
        let origin = match self.entries[index].state {
            State::Waiting { ready_at, .. } if self.entries[index].remaining.is_some() => ready_at,
            State::Waiting { .. } => self.entries[index].job.release_at,
            _ => time,
        };
        let age = time.saturating_sub(origin) / self.aging_ms;
        (u64::from(base).saturating_add(age).min(9)) as u8
    }
}

fn valid_token(value: &str, min_len: usize, max_len: usize) -> bool {
    let bytes = value.as_bytes();
    (min_len..=max_len).contains(&bytes.len())
        && bytes
            .iter()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || *b == b'-')
}