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
}

pub struct Pool {
    jobs: Vec<Job>,
    dependencies: Vec<Vec<usize>>,
    states: Vec<Runtime>,
    by_id: HashMap<String, usize>,
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

        let mut by_id = HashMap::new();

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
            if by_id.contains_key(&job.id) {
                return Err(Error::Duplicate(i));
            }
            by_id.insert(job.id.clone(), i);
        }

        let mut dependencies = vec![Vec::new(); jobs.len()];
        for (i, job) in jobs.iter().enumerate() {
            let mut seen = HashSet::new();
            for (j, dep) in job.dependencies.iter().enumerate() {
                if !seen.insert(dep.as_str()) {
                    return Err(Error::DuplicateDependency(i, j));
                }
                match by_id.get(dep) {
                    Some(&index) => dependencies[i].push(index),
                    None => return Err(Error::MissingDependency(i, j)),
                }
            }
        }

        if has_cycle(&dependencies) {
            return Err(Error::Cycle);
        }

        let states = jobs
            .iter()
            .map(|job| Runtime {
                state: State::Waiting {
                    ready_at: job.release_at,
                    attempt: 0,
                },
                remaining: None,
                started_at: 0,
            })
            .collect();

        Ok(Self {
            jobs,
            dependencies,
            states,
            by_id,
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

        let mut lines = Vec::new();
        let mut time = if self.has_run {
            self.next_event_after(self.horizon, until)
        } else {
            Some(0)
        };

        while let Some(t) = time {
            self.process_timestamp(t, &mut lines);
            time = self.next_event_after(t, until);
        }

        self.horizon = until;
        self.has_run = true;
        Ok(lines)
    }

    pub fn status(&self, id: &str) -> Result<State, Error> {
        let &index = self.by_id.get(id).ok_or(Error::Unknown)?;
        Ok(self.states[index].state.clone())
    }

    fn next_event_after(&self, after: u64, until: u64) -> Option<u64> {
        let mut next: Option<u64> = None;

        for runtime in &self.states {
            let event = match runtime.state {
                State::Running { finish_at, .. } if finish_at > after => Some(finish_at),
                State::Waiting { ready_at, .. } if ready_at > after => Some(ready_at),
                _ => None,
            };
            if let Some(event) = event.filter(|&event| event <= until) {
                next = Some(next.map_or(event, |n| n.min(event)));
            }
        }
        next
    }

    fn process_timestamp(&mut self, time: u64, lines: &mut Vec<String>) {
        let mut completed: Vec<usize> = self
            .states
            .iter()
            .enumerate()
            .filter_map(|(i, runtime)| match runtime.state {
                State::Running { finish_at, .. } if finish_at == time => Some(i),
                _ => None,
            })
            .collect();

        completed.sort_by(|&a, &b| self.jobs[a].id.as_bytes().cmp(self.jobs[b].id.as_bytes()));

        for index in completed {
            let attempt = match self.states[index].state {
                State::Running { attempt, .. } => attempt,
                _ => continue,
            };
            let id = &self.jobs[index].id;
            if attempt > self.jobs[index].failures {
                lines.push(format!("{time} finish {id} {attempt} ok"));
                self.states[index].state = State::Succeeded;
                self.states[index].remaining = None;
            } else {
                lines.push(format!("{time} finish {id} {attempt} fail"));
                self.states[index].remaining = None;
                if attempt >= self.jobs[index].max_attempts {
                    self.states[index].state = State::Failed;
                } else {
                    let shift = u32::from(attempt.saturating_sub(1));
                    let delay = self
                        .retry_base
                        .checked_mul(1u64.checked_shl(shift).unwrap_or(u64::MAX))
                        .unwrap_or(u64::MAX)
                        .min(self.retry_cap);
                    let ready_at = time.saturating_add(delay);
                    self.states[index].state = State::Waiting { ready_at, attempt };
                    lines.push(format!("{time} retry {id} {attempt} {ready_at}"));
                }
            }
        }

        self.propagate_blocked(time, lines);
        self.schedule(time, lines);
    }

    fn propagate_blocked(&mut self, time: u64, lines: &mut Vec<String>) {
        let mut newly_blocked = Vec::new();

        loop {
            let mut changed = false;
            for i in 0..self.jobs.len() {
                if !matches!(self.states[i].state, State::Waiting { .. }) {
                    continue;
                }
                let failed_dependency = self.dependencies[i].iter().copied().find(|&dep| {
                    matches!(
                        self.states[dep].state,
                        State::Failed | State::Blocked
                    )
                });
                if let Some(dep) = failed_dependency {
                    self.states[i].state = State::Blocked;
                    newly_blocked.push((i, dep));
                    changed = true;
                }
            }
            if !changed {
                break;
            }
        }

        newly_blocked.sort_by(|(a, _), (b, _)| {
            self.jobs[*a].id.as_bytes().cmp(self.jobs[*b].id.as_bytes())
        });

        for (job, dependency) in newly_blocked {
            lines.push(format!(
                "{time} skip {} {}",
                self.jobs[job].id, self.jobs[dependency].id
            ));
        }
    }

    fn schedule(&mut self, time: u64, lines: &mut Vec<String>) {
        loop {
            let mut candidates: Vec<usize> = (0..self.jobs.len())
                .filter(|&i| {
                    matches!(
                        self.states[i].state,
                        State::Waiting { ready_at, .. }
                            if ready_at <= time
                                && self.dependencies[i].iter().all(|&d| {
                                    matches!(self.states[d].state, State::Succeeded)
                                })
                    )
                })
                .collect();

            candidates.sort_by(|&a, &b| {
                let pa = self.effective_priority(a, time);
                let pb = self.effective_priority(b, time);
                pb.cmp(&pa).then_with(|| a.cmp(&b))
            });

            let mut scheduled = false;
            for candidate in candidates {
                let tag = self.jobs[candidate].tag.as_deref();
                let running: Vec<usize> = self
                    .states
                    .iter()
                    .enumerate()
                    .filter_map(|(i, runtime)| {
                        matches!(runtime.state, State::Running { .. }).then_some(i)
                    })
                    .collect();

                let tag_free = tag.is_none_or(|candidate_tag| {
                    running
                        .iter()
                        .all(|&i| self.jobs[i].tag.as_deref() != Some(candidate_tag))
                });

                if running.len() < self.capacity && tag_free {
                    self.start_job(candidate, time, lines, None);
                    scheduled = true;
                    break;
                }

                let mut victims: Vec<usize> = running
                    .into_iter()
                    .filter(|&victim| {
                        self.jobs[candidate].priority
                            >= self.jobs[victim].priority.saturating_add(2)
                    })
                    .filter(|&victim| {
                        let remaining_tag_users = self.states.iter().enumerate().any(|(i, rt)| {
                            i != victim
                                && matches!(rt.state, State::Running { .. })
                                && tag.is_some()
                                && self.jobs[i].tag.as_deref() == tag
                        });
                        !remaining_tag_users
                    })
                    .collect();

                victims.sort_by(|&a, &b| {
                    self.jobs[a]
                        .priority
                        .cmp(&self.jobs[b].priority)
                        .then_with(|| {
                            let ta = match self.states[a].state {
                                State::Running { .. } => self.states[a].started_at,
                                _ => 0,
                            };
                            let tb = match self.states[b].state {
                                State::Running { .. } => self.states[b].started_at,
                                _ => 0,
                            };
                            tb.cmp(&ta)
                        })
                        .then_with(|| {
                            self.jobs[b].id.as_bytes().cmp(self.jobs[a].id.as_bytes())
                        })
                });

                if let Some(victim) = victims.first().copied() {
                    let finish_at = match self.states[victim].state {
                        State::Running { finish_at, .. } => finish_at,
                        _ => continue,
                    };
                    let remaining = finish_at.saturating_sub(time);
                    let victim_id = self.jobs[victim].id.clone();
                    let candidate_id = self.jobs[candidate].id.clone();
                    let attempt = match self.states[victim].state {
                        State::Running { attempt, .. } => attempt,
                        _ => 0,
                    };

                    lines.push(format!(
                        "{time} preempt {victim_id} by {candidate_id}"
                    ));
                    self.states[victim].state = State::Waiting {
                        ready_at: time,
                        attempt,
                    };
                    self.states[victim].remaining = Some(remaining);
                    self.start_job(candidate, time, lines, None);
                    scheduled = true;
                    break;
                }
            }

            if !scheduled {
                break;
            }
        }
    }

    fn effective_priority(&self, index: usize, time: u64) -> u8 {
        if self.aging_ms == 0 {
            return self.jobs[index].priority;
        }
        let since = match self.states[index].state {
            State::Waiting { ready_at, .. } => ready_at,
            _ => time,
        };
        let gained = time.saturating_sub(since) / self.aging_ms;
        (u64::from(self.jobs[index].priority) + gained).min(9) as u8
    }

    fn start_job(
        &mut self,
        index: usize,
        time: u64,
        lines: &mut Vec<String>,
        _unused: Option<usize>,
    ) {
        let (attempt, remaining) = match self.states[index].state {
            State::Waiting { attempt, .. } => {
                if let Some(remaining) = self.states[index].remaining.take() {
                    (attempt, remaining)
                } else {
                    let next_attempt = attempt.saturating_add(1);
                    (next_attempt, self.jobs[index].duration)
                }
            }
            _ => return,
        };

        let finish_at = time.saturating_add(remaining);
        self.states[index].state = State::Running { attempt, finish_at };
        self.states[index].started_at = time;
        lines.push(format!(
            "{time} start {} {attempt} {remaining}",
            self.jobs[index].id
        ));
    }
}

fn valid_token(value: &str, min: usize, max: usize) -> bool {
    let bytes = value.as_bytes();
    (min..=max).contains(&bytes.len())
        && bytes
            .iter()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || *b == b'-')
}

fn has_cycle(dependencies: &[Vec<usize>]) -> bool {
    fn visit(node: usize, deps: &[Vec<usize>], marks: &mut [u8]) -> bool {
        if marks[node] == 1 {
            return true;
        }
        if marks[node] == 2 {
            return false;
        }
        marks[node] = 1;
        for &dep in &deps[node] {
            if visit(dep, deps, marks) {
                return true;
            }
        }
        marks[node] = 2;
        false
    }

    let mut marks = vec![0; dependencies.len()];
    for node in 0..dependencies.len() {
        if visit(node, dependencies, &mut marks) {
            return true;
        }
    }
    false
}