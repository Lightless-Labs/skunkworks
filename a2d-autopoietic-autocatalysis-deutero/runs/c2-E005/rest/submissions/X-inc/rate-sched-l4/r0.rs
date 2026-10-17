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
enum Phase {
    Waiting {
        ready_at: u64,
        attempt: u8,
        remaining: Option<u64>,
    },
    Running {
        attempt: u8,
        finish_at: u64,
        started_at: u64,
    },
    Succeeded,
    Failed,
    Blocked,
}

pub struct Pool {
    jobs: Vec<Job>,
    deps: Vec<Vec<usize>>,
    states: Vec<Phase>,
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
                .as_deref()
                .is_some_and(|tag| !valid_name(tag, 1, 8))
            {
                return Err(Error::BadTag(i));
            }
            if ids.insert(job.id.clone(), i).is_some() {
                return Err(Error::Duplicate(i));
            }
        }

        let mut deps = vec![Vec::new(); jobs.len()];
        for (i, job) in jobs.iter().enumerate() {
            let mut seen = HashSet::new();
            for (j, spelling) in job.dependencies.iter().enumerate() {
                if !seen.insert(spelling.as_str()) {
                    return Err(Error::DuplicateDependency(i, j));
                }
                match ids.get(spelling) {
                    Some(&target) => deps[i].push(target),
                    None => return Err(Error::MissingDependency(i, j)),
                }
            }
        }

        let mut color = vec![0u8; jobs.len()];
        fn visit(node: usize, deps: &[Vec<usize>], color: &mut [u8]) -> bool {
            if color[node] == 1 {
                return false;
            }
            if color[node] == 2 {
                return true;
            }
            color[node] = 1;
            for &d in &deps[node] {
                if !visit(d, deps, color) {
                    return false;
                }
            }
            color[node] = 2;
            true
        }
        for i in 0..jobs.len() {
            if !visit(i, &deps, &mut color) {
                return Err(Error::Cycle);
            }
        }

        let states = jobs
            .iter()
            .map(|job| Phase::Waiting {
                ready_at: job.release_at,
                attempt: 0,
                remaining: None,
            })
            .collect();

        Ok(Self {
            jobs,
            deps,
            states,
            capacity,
            aging_ms,
            retry_base,
            retry_cap,
            horizon: 0,
            has_run: false,
        })
    }

    pub fn status(&self, id: &str) -> Result<State, Error> {
        let index = self
            .jobs
            .iter()
            .position(|job| job.id == id)
            .ok_or(Error::Unknown)?;
        Ok(match self.states[index] {
            Phase::Waiting {
                ready_at, attempt, ..
            } => State::Waiting { ready_at, attempt },
            Phase::Running {
                attempt, finish_at, ..
            } => State::Running { attempt, finish_at },
            Phase::Succeeded => State::Succeeded,
            Phase::Failed => State::Failed,
            Phase::Blocked => State::Blocked,
        })
    }

    pub fn run(&mut self, until: u64) -> Result<Vec<String>, Error> {
        if until < self.horizon {
            return Err(Error::TimeReversed);
        }
        if until > 1_000_000_000 {
            return Err(Error::TimeRange);
        }

        let mut trace = Vec::new();
        let mut time = if self.has_run {
            self.next_event_after(self.horizon)
        } else {
            Some(0)
        };

        while let Some(t) = time {
            if t > until {
                break;
            }
            self.process_time(t, &mut trace);
            time = self.next_event_after(t);
        }

        self.horizon = until;
        self.has_run = true;
        Ok(trace)
    }

    fn next_event_after(&self, time: u64) -> Option<u64> {
        self.states
            .iter()
            .filter_map(|state| match state {
                Phase::Waiting { ready_at, .. } if *ready_at > time => Some(*ready_at),
                Phase::Running { finish_at, .. } if *finish_at > time => Some(*finish_at),
                _ => None,
            })
            .min()
    }

    fn process_time(&mut self, time: u64, trace: &mut Vec<String>) {
        let mut completing: Vec<usize> = self
            .states
            .iter()
            .enumerate()
            .filter_map(|(i, state)| match state {
                Phase::Running { finish_at, .. } if *finish_at == time => Some(i),
                _ => None,
            })
            .collect();
        completing.sort_by(|&a, &b| self.jobs[a].id.cmp(&self.jobs[b].id));

        for i in completing {
            let attempt = match self.states[i] {
                Phase::Running { attempt, .. } => attempt,
                _ => continue,
            };
            if attempt > self.jobs[i].failures {
                self.states[i] = Phase::Succeeded;
                trace.push(format!("{} finish {} {} ok", time, self.jobs[i].id, attempt));
            } else {
                trace.push(format!("{} finish {} {} fail", time, self.jobs[i].id, attempt));
                if attempt >= self.jobs[i].max_attempts {
                    self.states[i] = Phase::Failed;
                } else {
                    let shift = u32::from(attempt.saturating_sub(1));
                    let delay = self
                        .retry_base
                        .checked_shl(shift)
                        .unwrap_or(u64::MAX)
                        .min(self.retry_cap);
                    let ready_at = time.saturating_add(delay);
                    self.states[i] = Phase::Waiting {
                        ready_at,
                        attempt,
                        remaining: None,
                    };
                    trace.push(format!(
                        "{} retry {} {} {}",
                        time, self.jobs[i].id, attempt, ready_at
                    ));
                }
            }
        }

        let mut newly_blocked = Vec::new();
        loop {
            let mut changed = false;
            for i in 0..self.jobs.len() {
                if !matches!(self.states[i], Phase::Waiting { .. }) {
                    continue;
                }
                if let Some(&failed_dep) = self.deps[i].iter().find(|&&d| {
                    matches!(self.states[d], Phase::Failed | Phase::Blocked)
                }) {
                    self.states[i] = Phase::Blocked;
                    newly_blocked.push((i, failed_dep));
                    changed = true;
                }
            }
            if !changed {
                break;
            }
        }
        newly_blocked.sort_by(|(a, _), (b, _)| self.jobs[*a].id.cmp(&self.jobs[*b].id));
        for (i, dep) in newly_blocked {
            trace.push(format!(
                "{} skip {} {}",
                time, self.jobs[i].id, self.jobs[dep].id
            ));
        }

        loop {
            let mut candidates: Vec<usize> = self
                .states
                .iter()
                .enumerate()
                .filter_map(|(i, state)| match state {
                    Phase::Waiting { ready_at, .. }
                        if *ready_at <= time
                            && self.deps[i]
                                .iter()
                                .all(|&d| matches!(self.states[d], Phase::Succeeded)) =>
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

            let mut started = false;
            for candidate in candidates {
                if self.can_start(candidate) {
                    self.start_job(candidate, time, trace);
                    started = true;
                    break;
                }

                let victim = self.choose_victim(candidate, time);
                if let Some(victim) = victim {
                    self.preempt(victim, candidate, time, trace);
                    self.start_job(candidate, time, trace);
                    started = true;
                    break;
                }
            }
            if !started {
                break;
            }
        }
    }

    fn effective_priority(&self, i: usize, time: u64) -> u8 {
        if self.aging_ms == 0 {
            return self.jobs[i].priority;
        }
        let ready_at = match self.states[i] {
            Phase::Waiting { ready_at, .. } => ready_at,
            _ => time,
        };
        let age = time.saturating_sub(ready_at) / self.aging_ms;
        (u64::from(self.jobs[i].priority) + age).min(9) as u8
    }

    fn running_indices(&self) -> Vec<usize> {
        self.states
            .iter()
            .enumerate()
            .filter_map(|(i, state)| matches!(state, Phase::Running { .. }).then_some(i))
            .collect()
    }

    fn can_start(&self, candidate: usize) -> bool {
        let running = self.running_indices();
        running.len() < self.capacity
            && self.tag_is_free(candidate, &running)
    }

    fn tag_is_free(&self, candidate: usize, running: &[usize]) -> bool {
        match self.jobs[candidate].tag.as_deref() {
            None => true,
            Some(tag) => !running
                .iter()
                .any(|&i| self.jobs[i].tag.as_deref() == Some(tag)),
        }
    }

    fn choose_victim(&self, candidate: usize, time: u64) -> Option<usize> {
        let running = self.running_indices();
        let mut victims: Vec<usize> = running
            .iter()
            .copied()
            .filter(|&victim| {
                let attempt_gap = i16::from(self.jobs[candidate].priority)
                    - i16::from(self.jobs[victim].priority);
                if attempt_gap < 2 {
                    return false;
                }
                let remaining_slots = running.len() - 1 < self.capacity;
                let mut remaining = running.clone();
                remaining.retain(|&i| i != victim);
                remaining_slots && self.tag_is_free(candidate, &remaining)
            })
            .collect();

        victims.sort_by(|&a, &b| {
            let a_start = match self.states[a] {
                Phase::Running { started_at, .. } => started_at,
                _ => 0,
            };
            let b_start = match self.states[b] {
                Phase::Running { started_at, .. } => started_at,
                _ => 0,
            };
            self.jobs[a]
                .priority
                .cmp(&self.jobs[b].priority)
                .then_with(|| b_start.cmp(&a_start))
                .then_with(|| self.jobs[b].id.cmp(&self.jobs[a].id))
        });

        let _ = time;
        victims.into_iter().next()
    }

    fn preempt(
        &mut self,
        victim: usize,
        candidate: usize,
        time: u64,
        trace: &mut Vec<String>,
    ) {
        let (attempt, finish_at) = match self.states[victim] {
            Phase::Running {
                attempt, finish_at, ..
            } => (attempt, finish_at),
            _ => return,
        };
        let remaining = finish_at.saturating_sub(time);
        self.states[victim] = Phase::Waiting {
            ready_at: time,
            attempt,
            remaining: Some(remaining),
        };
        trace.push(format!(
            "{} preempt {} by {}",
            time, self.jobs[victim].id, self.jobs[candidate].id
        ));
    }

    fn start_job(&mut self, i: usize, time: u64, trace: &mut Vec<String>) {
        let (attempt, remaining) = match self.states[i] {
            Phase::Waiting {
                attempt, remaining, ..
            } => match remaining {
                Some(left) => (attempt, left),
                None => (attempt.saturating_add(1), self.jobs[i].duration),
            },
            _ => return,
        };
        let finish_at = time.saturating_add(remaining);
        self.states[i] = Phase::Running {
            attempt,
            finish_at,
            started_at: time,
        };
        trace.push(format!(
            "{} start {} {} {}",
            time, self.jobs[i].id, attempt, remaining
        ));
    }
}

fn valid_name(value: &str, min: usize, max: usize) -> bool {
    (min..=max).contains(&value.len())
        && value
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
}