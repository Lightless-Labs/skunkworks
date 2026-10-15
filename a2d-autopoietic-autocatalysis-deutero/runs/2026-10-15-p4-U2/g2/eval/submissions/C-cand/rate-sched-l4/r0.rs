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
enum InternalState {
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
    states: Vec<InternalState>,
    dependencies: Vec<Vec<usize>>,
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
            if !valid_atom(&job.id, 1, 16) {
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
                if !valid_atom(tag, 1, 8) {
                    return Err(Error::BadTag(i));
                }
            }
            if ids.insert(job.id.as_str(), i).is_some() {
                return Err(Error::Duplicate(i));
            }
        }

        let mut dependencies = vec![Vec::new(); jobs.len()];
        for (i, job) in jobs.iter().enumerate() {
            let mut seen = HashSet::new();
            for (j, name) in job.dependencies.iter().enumerate() {
                if !seen.insert(name.as_str()) {
                    return Err(Error::DuplicateDependency(i, j));
                }
                match ids.get(name.as_str()) {
                    Some(&index) => dependencies[i].push(index),
                    None => return Err(Error::MissingDependency(i, j)),
                }
            }
        }

        let mut marks = vec![0u8; jobs.len()];
        for i in 0..jobs.len() {
            if visit_cycle(i, &dependencies, &mut marks) {
                return Err(Error::Cycle);
            }
        }

        let states = jobs
            .iter()
            .map(|job| InternalState::Waiting {
                ready_at: job.release_at,
                attempt: 0,
                remaining: None,
            })
            .collect();

        Ok(Self {
            jobs,
            states,
            dependencies,
            capacity,
            aging_ms,
            retry_base,
            retry_cap,
            horizon: 0,
            has_run: false,
        })
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

        let mut states = self.states.clone();
        let mut trace = Vec::new();
        let mut time = if self.has_run {
            self.horizon
        } else {
            0
        };

        loop {
            if self.has_run && time == self.horizon {
                // The previous horizon was already processed. Continue only
                // if a future event lies at or before this call's horizon.
                let next = next_event(&states, time);
                match next {
                    Some(t) if t <= until && t > time => time = t,
                    _ => break,
                }
            }

            process_completions(
                time,
                &self.jobs,
                &mut states,
                self.retry_base,
                self.retry_cap,
                &mut trace,
            );

            propagate_blocks(time, &self.jobs, &self.dependencies, &mut states, &mut trace);

            schedule(
                time,
                &self.jobs,
                &self.dependencies,
                &mut states,
                self.capacity,
                self.aging_ms,
                &mut trace,
            );

            let next = next_event(&states, time);
            match next {
                Some(t) if t <= until && t > time => time = t,
                _ => break,
            }
        }

        self.states = states;
        self.horizon = until;
        self.has_run = true;
        Ok(trace)
    }

    pub fn status(&self, id: &str) -> Result<State, Error> {
        let index = self
            .jobs
            .iter()
            .position(|job| job.id == id)
            .ok_or(Error::Unknown)?;

        Ok(match &self.states[index] {
            InternalState::Waiting {
                ready_at, attempt, ..
            } => State::Waiting {
                ready_at: *ready_at,
                attempt: *attempt,
            },
            InternalState::Running {
                attempt, finish_at, ..
            } => State::Running {
                attempt: *attempt,
                finish_at: *finish_at,
            },
            InternalState::Succeeded => State::Succeeded,
            InternalState::Failed => State::Failed,
            InternalState::Blocked => State::Blocked,
        })
    }
}

fn valid_atom(value: &str, min: usize, max: usize) -> bool {
    let bytes = value.as_bytes();
    (min..=max).contains(&bytes.len())
        && bytes
            .iter()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || *b == b'-')
}

fn visit_cycle(index: usize, dependencies: &[Vec<usize>], marks: &mut [u8]) -> bool {
    if marks[index] == 1 {
        return true;
    }
    if marks[index] == 2 {
        return false;
    }
    marks[index] = 1;
    for &dep in &dependencies[index] {
        if visit_cycle(dep, dependencies, marks) {
            return true;
        }
    }
    marks[index] = 2;
    false
}

fn next_event(states: &[InternalState], now: u64) -> Option<u64> {
    let mut next = None;
    for state in states {
        let time = match state {
            InternalState::Waiting { ready_at, .. } => Some(*ready_at),
            InternalState::Running { finish_at, .. } => Some(*finish_at),
            _ => None,
        };
        if let Some(time) = time {
            if time > now && next.map_or(true, |current| time < current) {
                next = Some(time);
            }
        }
    }
    next
}

fn process_completions(
    time: u64,
    jobs: &[Job],
    states: &mut [InternalState],
    retry_base: u64,
    retry_cap: u64,
    trace: &mut Vec<String>,
) {
    let mut completed: Vec<usize> = states
        .iter()
        .enumerate()
        .filter_map(|(i, state)| match state {
            InternalState::Running { finish_at, .. } if *finish_at == time => Some(i),
            _ => None,
        })
        .collect();
    completed.sort_by(|a, b| jobs[*a].id.as_bytes().cmp(jobs[*b].id.as_bytes()));

    for i in completed {
        let attempt = match states[i] {
            InternalState::Running { attempt, .. } => attempt,
            _ => continue,
        };
        let ok = attempt > jobs[i].failures;
        trace.push(format!(
            "{} finish {} {} {}",
            time,
            jobs[i].id,
            attempt,
            if ok { "ok" } else { "fail" }
        ));

        if ok {
            states[i] = InternalState::Succeeded;
        } else if attempt >= jobs[i].max_attempts {
            states[i] = InternalState::Failed;
        } else {
            let shift = u32::from(attempt.saturating_sub(1));
            let delay = retry_base
                .checked_shl(shift)
                .unwrap_or(u64::MAX)
                .min(retry_cap);
            let ready_at = time.saturating_add(delay);
            trace.push(format!(
                "{} retry {} {} {}",
                time, jobs[i].id, attempt, ready_at
            ));
            states[i] = InternalState::Waiting {
                ready_at,
                attempt,
                remaining: None,
            };
        }
    }
}

fn propagate_blocks(
    time: u64,
    jobs: &[Job],
    dependencies: &[Vec<usize>],
    states: &mut [InternalState],
    trace: &mut Vec<String>,
) {
    let mut newly_blocked = Vec::new();
    loop {
        let mut changed = Vec::new();
        for i in 0..jobs.len() {
            if !matches!(states[i], InternalState::Waiting { .. }) {
                continue;
            }
            if dependencies[i].iter().any(|&dep| {
                matches!(states[dep], InternalState::Failed | InternalState::Blocked)
            }) {
                changed.push(i);
            }
        }
        if changed.is_empty() {
            break;
        }
        for i in changed {
            states[i] = InternalState::Blocked;
            newly_blocked.push(i);
        }
    }
    newly_blocked.sort_by(|a, b| jobs[*a].id.as_bytes().cmp(jobs[*b].id.as_bytes()));
    for i in newly_blocked {
        let failed_dep = dependencies[i]
            .iter()
            .copied()
            .find(|&dep| {
                matches!(states[dep], InternalState::Failed | InternalState::Blocked)
            })
            .unwrap_or(i);
        trace.push(format!(
            "{} skip {} {}",
            time, jobs[i].id, jobs[failed_dep].id
        ));
    }
}

fn schedule(
    time: u64,
    jobs: &[Job],
    dependencies: &[Vec<usize>],
    states: &mut [InternalState],
    capacity: usize,
    aging_ms: u64,
    trace: &mut Vec<String>,
) {
    loop {
        let mut candidates: Vec<usize> = (0..jobs.len())
            .filter(|&i| {
                match states[i] {
                    InternalState::Waiting { ready_at, .. } => {
                        ready_at <= time
                            && dependencies[i]
                                .iter()
                                .all(|&dep| matches!(states[dep], InternalState::Succeeded))
                    }
                    _ => false,
                }
            })
            .collect();

        candidates.sort_by(|a, b| {
            let pa = effective_priority(&jobs[*a], waiting_ready_at(&states[*a]), time, aging_ms);
            let pb = effective_priority(&jobs[*b], waiting_ready_at(&states[*b]), time, aging_ms);
            pb.cmp(&pa).then_with(|| a.cmp(b))
        });

        let mut action = None;
        for candidate in candidates {
            let running: Vec<usize> = states
                .iter()
                .enumerate()
                .filter_map(|(i, state)| matches!(state, InternalState::Running { .. }).then_some(i))
                .collect();
            let candidate_tag = jobs[candidate].tag.as_deref();
            let tag_free = candidate_tag.map_or(true, |tag| {
                !running
                    .iter()
                    .any(|&i| jobs[i].tag.as_deref() == Some(tag))
            });

            if running.len() < capacity && tag_free {
                action = Some((candidate, None));
                break;
            }

            let mut victims: Vec<usize> = running
                .iter()
                .copied()
                .filter(|&victim| {
                    jobs[candidate].priority >= jobs[victim].priority.saturating_add(2)
                        && running.len() - 1 < capacity
                        && candidate_tag.map_or(true, |tag| {
                            !running.iter().any(|&other| {
                                other != victim && jobs[other].tag.as_deref() == Some(tag)
                            })
                        })
                })
                .collect();

            victims.sort_by(|a, b| {
                jobs[*a]
                    .priority
                    .cmp(&jobs[*b].priority)
                    .then_with(|| {
                        let sa = running_started_at(&states[*a]);
                        let sb = running_started_at(&states[*b]);
                        sb.cmp(&sa)
                    })
                    .then_with(|| jobs[*b].id.as_bytes().cmp(jobs[*a].id.as_bytes()))
            });

            if let Some(victim) = victims.first().copied() {
                action = Some((candidate, Some(victim)));
                break;
            }
        }

        let Some((candidate, victim)) = action else {
            break;
        };

        if let Some(victim) = victim {
            let remaining = match states[victim] {
                InternalState::Running { finish_at, .. } => finish_at - time,
                _ => 0,
            };
            let attempt = match states[victim] {
                InternalState::Running { attempt, .. } => attempt,
                _ => 0,
            };
            trace.push(format!(
                "{} preempt {} by {}",
                time, jobs[victim].id, jobs[candidate].id
            ));
            states[victim] = InternalState::Waiting {
                ready_at: time,
                attempt,
                remaining: Some(remaining),
            };
        }

        let (previous_attempt, remaining) = match states[candidate] {
            InternalState::Waiting {
                attempt, remaining, ..
            } => (attempt, remaining),
            _ => (0, None),
        };
        let is_resume = remaining.is_some();
        let attempt = if is_resume {
            previous_attempt
        } else {
            previous_attempt.saturating_add(1)
        };
        let duration = remaining.unwrap_or(jobs[candidate].duration);
        let finish_at = time.saturating_add(duration);
        trace.push(format!(
            "{} start {} {} {}",
            time, jobs[candidate].id, attempt, duration
        ));
        states[candidate] = InternalState::Running {
            attempt,
            finish_at,
            started_at: time,
        };
    }
}

fn waiting_ready_at(state: &InternalState) -> u64 {
    match state {
        InternalState::Waiting { ready_at, .. } => *ready_at,
        _ => 0,
    }
}

fn running_started_at(state: &InternalState) -> u64 {
    match state {
        InternalState::Running { started_at, .. } => *started_at,
        _ => 0,
    }
}

fn effective_priority(job: &Job, ready_at: u64, time: u64, aging_ms: u64) -> u8 {
    if aging_ms == 0 {
        return job.priority;
    }
    let age = time.saturating_sub(ready_at) / aging_ms;
    (u64::from(job.priority) + age).min(9) as u8
}

fn main() {}