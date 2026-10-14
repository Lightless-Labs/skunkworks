use std::collections::HashMap;

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

struct RuntimeJob {
    job: Job,
    state: State,
    remaining: Option<u64>,
    segment_order: u64,
}

pub struct Pool {
    jobs: Vec<RuntimeJob>,
    ids: HashMap<String, usize>,
    dependencies: Vec<Vec<usize>>,
    capacity: usize,
    aging_ms: u64,
    retry_base: u64,
    retry_cap: u64,
    horizon: u64,
    initialized: bool,
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

        let mut ids = HashMap::new();
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
            if let Some(tag) = &job.tag {
                if !valid_id(tag) || tag.len() > 8 {
                    return Err(Error::BadTag(i));
                }
            }
            if ids.insert(job.id.clone(), i).is_some() {
                return Err(Error::Duplicate(i));
            }
        }

        let mut dependencies = Vec::with_capacity(jobs.len());
        for (i, job) in jobs.iter().enumerate() {
            let mut seen = HashMap::new();
            let mut resolved = Vec::with_capacity(job.dependencies.len());
            for (j, dep) in job.dependencies.iter().enumerate() {
                if seen.insert(dep, ()).is_some() {
                    return Err(Error::DuplicateDependency(i, j));
                }
                match ids.get(dep) {
                    Some(&index) => resolved.push(index),
                    None => return Err(Error::MissingDependency(i, j)),
                }
            }
            dependencies.push(resolved);
        }

        if has_cycle(&dependencies) {
            return Err(Error::Cycle);
        }

        let runtime_jobs = jobs
            .into_iter()
            .map(|job| RuntimeJob {
                state: State::Waiting {
                    ready_at: job.release_at,
                    attempt: 0,
                },
                job,
                remaining: None,
                segment_order: 0,
            })
            .collect();

        Ok(Self {
            jobs: runtime_jobs,
            ids,
            dependencies,
            capacity,
            aging_ms,
            retry_base,
            retry_cap,
            horizon: 0,
            initialized: false,
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

        if !self.initialized {
            self.process_timestamp(0, &mut lines);
            self.initialized = true;
        }

        let mut cursor = self.horizon;
        while let Some(time) = self.next_event_after(cursor, until) {
            self.process_timestamp(time, &mut lines);
            cursor = time;
        }

        self.horizon = until;
        Ok(lines)
    }

    pub fn status(&self, id: &str) -> Result<State, Error> {
        let index = *self.ids.get(id).ok_or(Error::Unknown)?;
        Ok(self.jobs[index].state.clone())
    }

    fn next_event_after(&self, cursor: u64, until: u64) -> Option<u64> {
        let mut next: Option<u64> = None;

        for runtime in &self.jobs {
            let time = match runtime.state {
                State::Running { finish_at, .. } => Some(finish_at),
                State::Waiting { ready_at, .. } => Some(ready_at),
                _ => None,
            };
            if let Some(time) = time {
                if time > cursor && time <= until {
                    next = Some(next.map_or(time, |current| current.min(time)));
                }
            }
        }

        next
    }

    fn process_timestamp(&mut self, time: u64, lines: &mut Vec<String>) {
        let mut completions: Vec<usize> = self
            .jobs
            .iter()
            .enumerate()
            .filter_map(|(i, runtime)| match runtime.state {
                State::Running { finish_at, .. } if finish_at == time => Some(i),
                _ => None,
            })
            .collect();

        completions.sort_by(|&a, &b| self.jobs[a].job.id.cmp(&self.jobs[b].job.id));

        for index in completions {
            let attempt = match self.jobs[index].state {
                State::Running { attempt, .. } => attempt,
                _ => continue,
            };
            let id = self.jobs[index].job.id.clone();

            if attempt > self.jobs[index].job.failures {
                lines.push(format!("{time} finish {id} {attempt} ok"));
                self.jobs[index].state = State::Succeeded;
                self.jobs[index].remaining = None;
            } else {
                lines.push(format!("{time} finish {id} {attempt} fail"));
                self.jobs[index].remaining = None;
                if attempt >= self.jobs[index].job.max_attempts {
                    self.jobs[index].state = State::Failed;
                } else {
                    let delay = self.retry_delay(attempt);
                    let ready_at = time.saturating_add(delay);
                    self.jobs[index].state = State::Waiting { ready_at, attempt };
                    lines.push(format!("{time} retry {id} {attempt} {ready_at}"));
                }
            }
        }

        self.propagate_failures(time, lines);
        self.schedule(time, lines);
    }

    fn retry_delay(&self, attempt: u8) -> u64 {
        let mut delay = self.retry_base;
        for _ in 1..attempt {
            delay = delay.saturating_mul(2).min(self.retry_cap);
        }
        delay.min(self.retry_cap)
    }

    fn propagate_failures(&mut self, time: u64, lines: &mut Vec<String>) {
        let mut newly_blocked: Vec<(usize, String)> = Vec::new();

        loop {
            let mut round = Vec::new();
            for i in 0..self.jobs.len() {
                if !matches!(self.jobs[i].state, State::Waiting { .. }) {
                    continue;
                }

                let failed_dependency = self.dependencies[i].iter().find_map(|&dep| {
                    if matches!(self.jobs[dep].state, State::Failed | State::Blocked) {
                        Some(self.jobs[dep].job.id.clone())
                    } else {
                        None
                    }
                });

                if let Some(dep_id) = failed_dependency {
                    round.push((i, dep_id));
                }
            }

            if round.is_empty() {
                break;
            }

            for (i, dep_id) in round {
                self.jobs[i].state = State::Blocked;
                self.jobs[i].remaining = None;
                newly_blocked.push((i, dep_id));
            }
        }

        newly_blocked.sort_by(|(a, _), (b, _)| self.jobs[*a].job.id.cmp(&self.jobs[*b].job.id));
        for (index, dependency_id) in newly_blocked {
            lines.push(format!(
                "{time} skip {} {dependency_id}",
                self.jobs[index].job.id
            ));
        }
    }

    fn schedule(&mut self, time: u64, lines: &mut Vec<String>) {
        loop {
            let mut candidates: Vec<usize> = (0..self.jobs.len())
                .filter(|&i| self.is_eligible(i, time))
                .collect();

            candidates.sort_by(|&a, &b| {
                let pa = self.effective_priority(a, time);
                let pb = self.effective_priority(b, time);
                pb.cmp(&pa).then_with(|| a.cmp(&b))
            });

            let mut selected: Option<(usize, Option<usize>)> = None;

            for candidate in candidates {
                if self.can_start_without_preemption(candidate) {
                    selected = Some((candidate, None));
                    break;
                }
                if let Some(victim) = self.find_victim(candidate) {
                    selected = Some((candidate, Some(victim)));
                    break;
                }
            }

            let Some((candidate, victim)) = selected else {
                break;
            };

            if let Some(victim) = victim {
                let remaining = match self.jobs[victim].state {
                    State::Running { finish_at, .. } => finish_at - time,
                    _ => 0,
                };
                let attempt = match self.jobs[victim].state {
                    State::Running { attempt, .. } => attempt,
                    _ => 0,
                };
                let victim_id = self.jobs[victim].job.id.clone();
                let candidate_id = self.jobs[candidate].job.id.clone();

                self.jobs[victim].state = State::Waiting {
                    ready_at: time,
                    attempt,
                };
                self.jobs[victim].remaining = Some(remaining);
                lines.push(format!(
                    "{time} preempt {victim_id} by {candidate_id}"
                ));
            }

            let (attempt, remaining) = if let Some(remaining) = self.jobs[candidate].remaining.take()
            {
                let attempt = match self.jobs[candidate].state {
                    State::Waiting { attempt, .. } => attempt,
                    _ => 0,
                };
                (attempt, remaining)
            } else {
                let previous = match self.jobs[candidate].state {
                    State::Waiting { attempt, .. } => attempt,
                    _ => 0,
                };
                (previous + 1, self.jobs[candidate].job.duration)
            };

            let finish_at = time.saturating_add(remaining);
            let id = self.jobs[candidate].job.id.clone();
            self.jobs[candidate].state = State::Running { attempt, finish_at };
            self.next_segment_order = self.next_segment_order.saturating_add(1);
            self.jobs[candidate].segment_order = self.next_segment_order;
            lines.push(format!("{time} start {id} {attempt} {remaining}"));
        }
    }

    fn is_eligible(&self, index: usize, time: u64) -> bool {
        let ready = match self.jobs[index].state {
            State::Waiting { ready_at, .. } => ready_at <= time,
            _ => false,
        };
        ready
            && self.dependencies[index]
                .iter()
                .all(|&dep| matches!(self.jobs[dep].state, State::Succeeded))
    }

    fn effective_priority(&self, index: usize, time: u64) -> u8 {
        if self.aging_ms == 0 {
            return self.jobs[index].job.priority;
        }

        let ready_at = match self.jobs[index].state {
            State::Waiting { ready_at, .. } => ready_at,
            _ => time,
        };
        let age = (time - ready_at) / self.aging_ms;
        (self.jobs[index].job.priority as u64 + age).min(9) as u8
    }

    fn can_start_without_preemption(&self, candidate: usize) -> bool {
        let running_count = self.running_count();
        if running_count >= self.capacity {
            return false;
        }
        let tag = self.jobs[candidate].job.tag.as_deref();
        !self.jobs.iter().any(|runtime| {
            matches!(runtime.state, State::Running { .. })
                && tag.is_some()
                && runtime.job.tag.as_deref() == tag
        })
    }

    fn find_victim(&self, candidate: usize) -> Option<usize> {
        let candidate_tag = self.jobs[candidate].job.tag.as_deref();
        let running_count = self.running_count();

        let mut victims: Vec<usize> = (0..self.jobs.len())
            .filter(|&i| {
                if !matches!(self.jobs[i].state, State::Running { .. }) {
                    return false;
                }
                if self.jobs[candidate].job.priority
                    < self.jobs[i].job.priority.saturating_add(2)
                {
                    return false;
                }

                let resulting_count = running_count - 1;
                if resulting_count >= self.capacity {
                    return false;
                }

                let candidate_tag_freed = candidate_tag.is_none()
                    || !self.jobs.iter().enumerate().any(|(other, runtime)| {
                        other != i
                            && matches!(runtime.state, State::Running { .. })
                            && runtime.job.tag.as_deref() == candidate_tag
                    });

                candidate_tag_freed
            })
            .collect();

        victims.sort_by(|&a, &b| {
            self.jobs[a]
                .job
                .priority
                .cmp(&self.jobs[b].job.priority)
                .then_with(|| {
                    self.jobs[b]
                        .segment_order
                        .cmp(&self.jobs[a].segment_order)
                })
                .then_with(|| self.jobs[b].job.id.cmp(&self.jobs[a].job.id))
        });

        victims.into_iter().next()
    }

    fn running_count(&self) -> usize {
        self.jobs
            .iter()
            .filter(|runtime| matches!(runtime.state, State::Running { .. }))
            .count()
    }
}

fn valid_id(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 16
        && value
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
}

fn has_cycle(dependencies: &[Vec<usize>]) -> bool {
    fn visit(node: usize, dependencies: &[Vec<usize>], marks: &mut [u8]) -> bool {
        if marks[node] == 1 {
            return true;
        }
        if marks[node] == 2 {
            return false;
        }

        marks[node] = 1;
        for &dependency in &dependencies[node] {
            if visit(dependency, dependencies, marks) {
                return true;
            }
        }
        marks[node] = 2;
        false
    }

    let mut marks = vec![0; dependencies.len()];
    (0..dependencies.len()).any(|node| visit(node, dependencies, &mut marks))
}