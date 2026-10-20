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
    segment_start: u64,
}

pub struct Pool {
    jobs: Vec<Job>,
    dependencies: Vec<Vec<usize>>,
    runtime: Vec<Runtime>,
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

        let mut ids = std::collections::HashMap::new();
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
            if job.tag.as_ref().is_some_and(|tag| !valid_name(tag, 1, 8)) {
                return Err(Error::BadTag(i));
            }
            if ids.insert(job.id.clone(), i).is_some() {
                return Err(Error::Duplicate(i));
            }
        }

        let mut dependencies = Vec::with_capacity(jobs.len());
        for (i, job) in jobs.iter().enumerate() {
            let mut seen = std::collections::HashSet::new();
            let mut resolved = Vec::with_capacity(job.dependencies.len());
            for (j, spelling) in job.dependencies.iter().enumerate() {
                if !seen.insert(spelling.as_str()) {
                    return Err(Error::DuplicateDependency(i, j));
                }
                match ids.get(spelling) {
                    Some(&index) => resolved.push(index),
                    None => return Err(Error::MissingDependency(i, j)),
                }
            }
            dependencies.push(resolved);
        }

        let mut marks = vec![0u8; jobs.len()];
        fn visit(node: usize, deps: &[Vec<usize>], marks: &mut [u8]) -> bool {
            if marks[node] == 1 {
                return false;
            }
            if marks[node] == 2 {
                return true;
            }
            marks[node] = 1;
            for &dependency in &deps[node] {
                if !visit(dependency, deps, marks) {
                    return false;
                }
            }
            marks[node] = 2;
            true
        }
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
                remaining: None,
                segment_start: 0,
            })
            .collect();

        Ok(Self {
            jobs,
            dependencies,
            runtime,
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

        let mut lines = Vec::new();
        let mut time = if self.first_run {
            0
        } else {
            self.horizon
        };
        let mut process_current = self.first_run;

        loop {
            if process_current {
                self.process_timestamp(time, &mut lines);
            }
            process_current = false;

            let next = self.next_event_after(time);
            match next {
                Some(next_time) if next_time <= until => {
                    time = next_time;
                    process_current = true;
                }
                _ => break,
            }
        }

        self.horizon = until;
        self.first_run = false;
        Ok(lines)
    }

    fn process_timestamp(&mut self, time: u64, lines: &mut Vec<String>) {
        let mut completions: Vec<usize> = self
            .runtime
            .iter()
            .enumerate()
            .filter_map(|(i, runtime)| match runtime.state {
                State::Running { finish_at, .. } if finish_at == time => Some(i),
                _ => None,
            })
            .collect();
        completions.sort_by(|&a, &b| self.jobs[a].id.cmp(&self.jobs[b].id));

        for i in completions {
            let attempt = match self.runtime[i].state {
                State::Running { attempt, .. } => attempt,
                _ => continue,
            };
            if attempt > self.jobs[i].failures {
                lines.push(format!("{time} finish {} {attempt} ok", self.jobs[i].id));
                self.runtime[i].state = State::Succeeded;
                self.runtime[i].remaining = None;
            } else {
                lines.push(format!("{time} finish {} {attempt} fail", self.jobs[i].id));
                self.runtime[i].remaining = None;
                if attempt >= self.jobs[i].max_attempts {
                    self.runtime[i].state = State::Failed;
                } else {
                    let shift = u32::from(attempt - 1);
                    let delay = self
                        .retry_base
                        .checked_shl(shift)
                        .unwrap_or(u64::MAX)
                        .min(self.retry_cap);
                    let ready_at = time.saturating_add(delay);
                    lines.push(format!("{time} retry {} {attempt} {ready_at}", self.jobs[i].id));
                    self.runtime[i].state = State::Waiting { ready_at, attempt };
                }
            }
        }

        let mut blocked = Vec::new();
        loop {
            let mut newly = Vec::new();
            for i in 0..self.jobs.len() {
                if !matches!(self.runtime[i].state, State::Waiting { .. }) {
                    continue;
                }
                if let Some(&failed_dep) = self.dependencies[i].iter().find(|&&dep| {
                    matches!(
                        self.runtime[dep].state,
                        State::Failed | State::Blocked
                    )
                }) {
                    newly.push((i, failed_dep));
                }
            }
            if newly.is_empty() {
                break;
            }
            for (i, dep) in newly {
                self.runtime[i].state = State::Blocked;
                blocked.push((i, dep));
            }
        }
        blocked.sort_by(|(a, _), (b, _)| self.jobs[*a].id.cmp(&self.jobs[*b].id));
        for (i, dep) in blocked {
            lines.push(format!(
                "{time} skip {} {}",
                self.jobs[i].id, self.jobs[dep].id
            ));
        }

        self.schedule(time, lines);
    }

    fn schedule(&mut self, time: u64, lines: &mut Vec<String>) {
        loop {
            let mut candidates: Vec<usize> = (0..self.jobs.len())
                .filter(|&i| {
                    matches!(
                        self.runtime[i].state,
                        State::Waiting { ready_at, .. }
                            if ready_at <= time
                                && self.dependencies[i].iter().all(|&d| {
                                    matches!(self.runtime[d].state, State::Succeeded)
                                })
                    )
                })
                .collect();
            candidates.sort_by(|&a, &b| {
                let pa = self.effective_priority(a, time);
                let pb = self.effective_priority(b, time);
                pb.cmp(&pa).then_with(|| a.cmp(&b))
            });

            let mut started = false;
            for candidate in candidates {
                let occupied = self.running_indices();
                let slot_free = occupied.len() < self.capacity;
                let tag_free = self.tag_is_free(candidate, &occupied);
                if slot_free && tag_free {
                    self.start(candidate, time, lines, None);
                    started = true;
                    break;
                }

                let mut victims: Vec<usize> = occupied
                    .into_iter()
                    .filter(|&victim| {
                        self.jobs[candidate].priority >= self.jobs[victim].priority.saturating_add(2)
                            && self.can_free_candidate_resources(candidate, victim)
                    })
                    .collect();
                victims.sort_by(|&a, &b| {
                    self.jobs[a]
                        .priority
                        .cmp(&self.jobs[b].priority)
                        .then_with(|| {
                            self.runtime[b]
                                .segment_start
                                .cmp(&self.runtime[a].segment_start)
                        })
                        .then_with(|| self.jobs[b].id.cmp(&self.jobs[a].id))
                });
                if let Some(victim) = victims.first().copied() {
                    let remaining = match self.runtime[victim].state {
                        State::Running { finish_at, .. } => finish_at - time,
                        _ => 0,
                    };
                    let attempt = match self.runtime[victim].state {
                        State::Running { attempt, .. } => attempt,
                        _ => 0,
                    };
                    lines.push(format!(
                        "{time} preempt {} by {}",
                        self.jobs[victim].id, self.jobs[candidate].id
                    ));
                    self.runtime[victim].state = State::Waiting {
                        ready_at: time,
                        attempt,
                    };
                    self.runtime[victim].remaining = Some(remaining);
                    self.start(candidate, time, lines, None);
                    started = true;
                    break;
                }
            }
            if !started {
                break;
            }
        }
    }

    fn start(&mut self, index: usize, time: u64, lines: &mut Vec<String>, _unused: Option<()>) {
        let (attempt, duration) = match self.runtime[index].state {
            State::Waiting { attempt, .. } => {
                if let Some(remaining) = self.runtime[index].remaining.take() {
                    (attempt, remaining)
                } else {
                    let next_attempt = attempt.saturating_add(1);
                    (next_attempt, self.jobs[index].duration)
                }
            }
            _ => return,
        };
        let finish_at = time.saturating_add(duration);
        lines.push(format!(
            "{time} start {} {attempt} {duration}",
            self.jobs[index].id
        ));
        self.runtime[index].state = State::Running { attempt, finish_at };
        self.runtime[index].segment_start = time;
    }

    fn running_indices(&self) -> Vec<usize> {
        self.runtime
            .iter()
            .enumerate()
            .filter_map(|(i, runtime)| matches!(runtime.state, State::Running { .. }).then_some(i))
            .collect()
    }

    fn tag_is_free(&self, candidate: usize, running: &[usize]) -> bool {
        match self.jobs[candidate].tag.as_deref() {
            None => true,
            Some(tag) => running
                .iter()
                .all(|&i| self.jobs[i].tag.as_deref() != Some(tag)),
        }
    }

    fn can_free_candidate_resources(&self, candidate: usize, victim: usize) -> bool {
        let running = self.running_indices();
        let after = running.len().saturating_sub(1);
        let slot_free = after < self.capacity;
        let tag_free = match self.jobs[candidate].tag.as_deref() {
            None => true,
            Some(tag) => running.iter().all(|&i| {
                i == victim || self.jobs[i].tag.as_deref() != Some(tag)
            }),
        };
        slot_free && tag_free
    }

    fn effective_priority(&self, index: usize, time: u64) -> u8 {
        if self.aging_ms == 0 {
            return self.jobs[index].priority;
        }
        let ready_at = match self.runtime[index].state {
            State::Waiting { ready_at, .. } => ready_at,
            _ => time,
        };
        let aged = (time - ready_at) / self.aging_ms;
        self.jobs[index]
            .priority
            .saturating_add(aged.min(9) as u8)
            .min(9)
    }

    fn next_event_after(&self, time: u64) -> Option<u64> {
        self.runtime
            .iter()
            .filter_map(|runtime| match runtime.state {
                State::Waiting { ready_at, .. } if ready_at > time => Some(ready_at),
                State::Running { finish_at, .. } if finish_at > time => Some(finish_at),
                _ => None,
            })
            .min()
    }
}

fn valid_name(value: &str, min: usize, max: usize) -> bool {
    let bytes = value.as_bytes();
    (min..=max).contains(&bytes.len())
        && bytes.iter().all(|&byte| {
            byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'-'
        })
}