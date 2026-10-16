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
    ready_since: u64,
    remaining: Option<u64>,
    segment_order: u64,
}

pub struct Pool {
    jobs: Vec<Job>,
    runtime: Vec<Runtime>,
    by_id: HashMap<String, usize>,
    deps: Vec<Vec<usize>>,
    dependents: Vec<Vec<usize>>,
    capacity: usize,
    aging_ms: u64,
    retry_base: u64,
    retry_cap: u64,
    horizon: u64,
    started_once: bool,
    sequence: u64,
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
            if job.id.is_empty()
                || job.id.len() > 16
                || !job
                    .id
                    .bytes()
                    .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
            {
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
                if tag.is_empty()
                    || tag.len() > 8
                    || !tag
                        .bytes()
                        .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
                {
                    return Err(Error::BadTag(i));
                }
            }
            if by_id.insert(job.id.clone(), i).is_some() {
                return Err(Error::Duplicate(i));
            }
        }

        let mut deps = vec![Vec::new(); jobs.len()];
        let mut dependents = vec![Vec::new(); jobs.len()];
        for (i, job) in jobs.iter().enumerate() {
            let mut seen = HashSet::new();
            for (j, spelling) in job.dependencies.iter().enumerate() {
                if !seen.insert(spelling.as_str()) {
                    return Err(Error::DuplicateDependency(i, j));
                }
                let Some(&dep) = by_id.get(spelling) else {
                    return Err(Error::MissingDependency(i, j));
                };
                deps[i].push(dep);
                dependents[dep].push(i);
            }
        }

        fn visit(node: usize, deps: &[Vec<usize>], marks: &mut [u8]) -> bool {
            if marks[node] == 1 {
                return false;
            }
            if marks[node] == 2 {
                return true;
            }
            marks[node] = 1;
            for &dep in &deps[node] {
                if !visit(dep, deps, marks) {
                    return false;
                }
            }
            marks[node] = 2;
            true
        }
        let mut marks = vec![0; jobs.len()];
        for i in 0..jobs.len() {
            if !visit(i, &deps, &mut marks) {
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
                ready_since: job.release_at,
                remaining: None,
                segment_order: 0,
            })
            .collect();

        Ok(Self {
            jobs,
            runtime,
            by_id,
            deps,
            dependents,
            capacity,
            aging_ms,
            retry_base,
            retry_cap,
            horizon: 0,
            started_once: false,
            sequence: 0,
        })
    }

    pub fn status(&self, id: &str) -> Result<State, Error> {
        self.by_id
            .get(id)
            .map(|&i| self.runtime[i].state.clone())
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
        let mut time = if self.started_once {
            self.next_event_after(self.horizon)
        } else {
            Some(0)
        };

        while let Some(t) = time {
            if t > until {
                break;
            }
            self.started_once = true;
            self.process_time(t, &mut lines);
            time = self.next_event_after(t);
        }
        self.horizon = until;
        Ok(lines)
    }

    fn next_event_after(&self, t: u64) -> Option<u64> {
        let mut next: Option<u64> = None;
        for rt in &self.runtime {
            let candidate = match rt.state {
                State::Running { finish_at, .. } if finish_at > t => Some(finish_at),
                State::Waiting { ready_at, .. } if ready_at > t => Some(ready_at),
                _ => None,
            };
            if let Some(x) = candidate {
                next = Some(next.map_or(x, |n| n.min(x)));
            }
        }
        next
    }

    fn process_time(&mut self, t: u64, lines: &mut Vec<String>) {
        let mut completed: Vec<usize> = self
            .runtime
            .iter()
            .enumerate()
            .filter_map(|(i, rt)| match rt.state {
                State::Running { finish_at, .. } if finish_at == t => Some(i),
                _ => None,
            })
            .collect();
        completed.sort_by(|&a, &b| self.jobs[a].id.as_bytes().cmp(self.jobs[b].id.as_bytes()));

        for i in completed {
            let attempt = match self.runtime[i].state {
                State::Running { attempt, .. } => attempt,
                _ => unreachable!(),
            };
            let ok = attempt > self.jobs[i].failures;
            lines.push(format!(
                "{} finish {} {} {}",
                t,
                self.jobs[i].id,
                attempt,
                if ok { "ok" } else { "fail" }
            ));
            self.runtime[i].remaining = None;
            self.runtime[i].state = if ok {
                State::Succeeded
            } else if attempt >= self.jobs[i].max_attempts {
                State::Failed
            } else {
                let shift = u32::from(attempt.saturating_sub(1));
                let delay = self
                    .retry_base
                    .checked_mul(1u64.checked_shl(shift).unwrap_or(u64::MAX))
                    .unwrap_or(u64::MAX)
                    .min(self.retry_cap);
                let ready = t.saturating_add(delay);
                lines.push(format!("{} retry {} {} {}", t, self.jobs[i].id, attempt, ready));
                self.runtime[i].ready_since = ready;
                State::Waiting {
                    ready_at: ready,
                    attempt,
                }
            };
        }

        loop {
            let mut newly_blocked = Vec::new();
            for i in 0..self.jobs.len() {
                if !matches!(self.runtime[i].state, State::Waiting { .. }) {
                    continue;
                }
                if let Some(&bad) = self.deps[i].iter().find(|&&d| {
                    matches!(self.runtime[d].state, State::Failed | State::Blocked)
                }) {
                    self.runtime[i].state = State::Blocked;
                    newly_blocked.push((i, bad));
                }
            }
            if newly_blocked.is_empty() {
                break;
            }
            newly_blocked.sort_by(|(a, _), (b, _)| {
                self.jobs[*a].id.as_bytes().cmp(self.jobs[*b].id.as_bytes())
            });
            for (i, dep) in newly_blocked {
                lines.push(format!("{} skip {} {}", t, self.jobs[i].id, self.jobs[dep].id));
            }
        }

        loop {
            let mut candidates: Vec<usize> = (0..self.jobs.len())
                .filter(|&i| {
                    matches!(self.runtime[i].state, State::Waiting { ready_at, .. } if ready_at <= t)
                        && self.deps[i]
                            .iter()
                            .all(|&d| matches!(self.runtime[d].state, State::Succeeded))
                })
                .collect();
            candidates.sort_by(|&a, &b| {
                let pa = self.effective_priority(a, t);
                let pb = self.effective_priority(b, t);
                pb.cmp(&pa).then_with(|| a.cmp(&b))
            });

            let mut action = None;
            for candidate in candidates {
                let running: Vec<usize> = self
                    .runtime
                    .iter()
                    .enumerate()
                    .filter_map(|(i, rt)| matches!(rt.state, State::Running { .. }).then_some(i))
                    .collect();
                let tag_free = self.jobs[candidate].tag.as_ref().is_none_or(|tag| {
                    !running.iter().any(|&i| self.jobs[i].tag.as_ref() == Some(tag))
                });
                if running.len() < self.capacity && tag_free {
                    action = Some((candidate, None));
                    break;
                }

                let mut victims: Vec<usize> = running
                    .into_iter()
                    .filter(|&v| {
                        self.jobs[candidate].priority >= self.jobs[v].priority.saturating_add(2)
                    })
                    .filter(|&v| {
                        let remaining_running = self
                            .runtime
                            .iter()
                            .enumerate()
                            .filter(|(i, rt)| {
                                *i != v && matches!(rt.state, State::Running { .. })
                            })
                            .map(|(i, _)| i);
                        let slot_free = self.runtime.iter().filter(|rt| {
                            matches!(rt.state, State::Running { .. })
                        }).count() - 1 < self.capacity;
                        let tag_free_after = self.jobs[candidate].tag.as_ref().is_none_or(|tag| {
                            !remaining_running
                                .into_iter()
                                .any(|i| self.jobs[i].tag.as_ref() == Some(tag))
                        });
                        slot_free && tag_free_after
                    })
                    .collect();
                victims.sort_by(|&a, &b| {
                    self.jobs[a]
                        .priority
                        .cmp(&self.jobs[b].priority)
                        .then_with(|| {
                            self.runtime[b]
                                .segment_order
                                .cmp(&self.runtime[a].segment_order)
                        })
                        .then_with(|| self.jobs[b].id.as_bytes().cmp(self.jobs[a].id.as_bytes()))
                });
                if let Some(&victim) = victims.first() {
                    action = Some((candidate, Some(victim)));
                    break;
                }
            }

            let Some((candidate, victim)) = action else {
                break;
            };
            if let Some(v) = victim {
                let (attempt, finish_at) = match self.runtime[v].state {
                    State::Running { attempt, finish_at } => (attempt, finish_at),
                    _ => unreachable!(),
                };
                let remaining = finish_at - t;
                lines.push(format!(
                    "{} preempt {} by {}",
                    t, self.jobs[v].id, self.jobs[candidate].id
                ));
                self.runtime[v].remaining = Some(remaining);
                self.runtime[v].ready_since = t;
                self.runtime[v].state = State::Waiting {
                    ready_at: t,
                    attempt,
                };
            }

            let (attempt, remaining) = match self.runtime[candidate].state {
                State::Waiting { attempt, .. } => {
                    if let Some(saved) = self.runtime[candidate].remaining.take() {
                        (attempt, saved)
                    } else {
                        let next_attempt = attempt.saturating_add(1);
                        (next_attempt, self.jobs[candidate].duration)
                    }
                }
                _ => unreachable!(),
            };
            self.sequence = self.sequence.saturating_add(1);
            self.runtime[candidate].segment_order = self.sequence;
            self.runtime[candidate].state = State::Running {
                attempt,
                finish_at: t.saturating_add(remaining),
            };
            lines.push(format!(
                "{} start {} {} {}",
                t, self.jobs[candidate].id, attempt, remaining
            ));
        }
    }

    fn effective_priority(&self, i: usize, t: u64) -> u8 {
        if self.aging_ms == 0 {
            return self.jobs[i].priority;
        }
        let age = t.saturating_sub(self.runtime[i].ready_since) / self.aging_ms;
        self.jobs[i]
            .priority
            .saturating_add(age.min(9) as u8)
            .min(9)
    }
}