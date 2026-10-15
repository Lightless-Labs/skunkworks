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
    ready_at: u64,
    started_at: u64,
    remaining: Option<u64>,
}

pub struct Pool {
    jobs: Vec<Job>,
    states: Vec<Runtime>,
    by_id: HashMap<String, usize>,
    horizon: u64,
    has_run: bool,
    capacity: usize,
    aging_ms: u64,
    retry_base: u64,
    retry_cap: u64,
}

fn valid_token(s: &str, min: usize, max: usize) -> bool {
    let b = s.as_bytes();
    b.len() >= min
        && b.len() <= max
        && b.iter()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || *c == b'-')
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

        let mut deps = vec![Vec::new(); jobs.len()];
        for (i, job) in jobs.iter().enumerate() {
            let mut seen = HashSet::new();
            for (j, dep) in job.dependencies.iter().enumerate() {
                if !seen.insert(dep.as_str()) {
                    return Err(Error::DuplicateDependency(i, j));
                }
                match by_id.get(dep) {
                    Some(&idx) => deps[i].push(idx),
                    None => return Err(Error::MissingDependency(i, j)),
                }
            }
        }

        fn visit(i: usize, deps: &[Vec<usize>], marks: &mut [u8]) -> bool {
            if marks[i] == 1 {
                return false;
            }
            if marks[i] == 2 {
                return true;
            }
            marks[i] = 1;
            for &d in &deps[i] {
                if !visit(d, deps, marks) {
                    return false;
                }
            }
            marks[i] = 2;
            true
        }

        let mut marks = vec![0; jobs.len()];
        for i in 0..jobs.len() {
            if !visit(i, &deps, &mut marks) {
                return Err(Error::Cycle);
            }
        }

        let states = jobs
            .iter()
            .map(|j| Runtime {
                state: State::Waiting {
                    ready_at: j.release_at,
                    attempt: 0,
                },
                ready_at: j.release_at,
                started_at: j.release_at,
                remaining: None,
            })
            .collect();

        Ok(Self {
            jobs,
            states,
            by_id,
            horizon: 0,
            has_run: false,
            capacity,
            aging_ms,
            retry_base,
            retry_cap,
        })
    }

    pub fn status(&self, id: &str) -> Result<State, Error> {
        let &i = self.by_id.get(id).ok_or(Error::Unknown)?;
        Ok(self.states[i].state.clone())
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

        let mut lines = Vec::new();
        let mut t = if self.has_run { self.horizon } else { 0 };
        let mut process_current = !self.has_run;

        loop {
            if process_current {
                self.process_time(t, &mut lines);
            }
            process_current = true;

            if t == until {
                break;
            }

            let mut next: Option<u64> = None;
            for r in &self.states {
                match r.state {
                    State::Running { finish_at, .. } if finish_at > t => {
                        next = Some(next.map_or(finish_at, |n| n.min(finish_at)));
                    }
                    State::Waiting { ready_at, .. } if ready_at > t => {
                        next = Some(next.map_or(ready_at, |n| n.min(ready_at)));
                    }
                    _ => {}
                }
            }

            match next {
                Some(n) if n <= until => t = n,
                _ => break,
            }
        }

        self.horizon = until;
        self.has_run = true;
        Ok(lines)
    }

    fn process_time(&mut self, t: u64, lines: &mut Vec<String>) {
        let mut finished: Vec<usize> = self
            .states
            .iter()
            .enumerate()
            .filter_map(|(i, r)| match r.state {
                State::Running { finish_at, .. } if finish_at == t => Some(i),
                _ => None,
            })
            .collect();
        finished.sort_by(|&a, &b| self.jobs[a].id.cmp(&self.jobs[b].id));

        for i in finished {
            let attempt = match self.states[i].state {
                State::Running { attempt, .. } => attempt,
                _ => continue,
            };
            if attempt > self.jobs[i].failures {
                lines.push(format!("{} finish {} {} ok", t, self.jobs[i].id, attempt));
                self.states[i].state = State::Succeeded;
                self.states[i].remaining = None;
            } else {
                lines.push(format!("{} finish {} {} fail", t, self.jobs[i].id, attempt));
                self.states[i].remaining = None;
                if attempt >= self.jobs[i].max_attempts {
                    self.states[i].state = State::Failed;
                } else {
                    let shift = u32::from(attempt.saturating_sub(1));
                    let delay = self
                        .retry_base
                        .checked_shl(shift)
                        .unwrap_or(u64::MAX)
                        .min(self.retry_cap);
                    let ready = t.saturating_add(delay);
                    lines.push(format!(
                        "{} retry {} {} {}",
                        t, self.jobs[i].id, attempt, ready
                    ));
                    self.states[i].state = State::Waiting {
                        ready_at: ready,
                        attempt,
                    };
                    self.states[i].ready_at = ready;
                    self.states[i].started_at = ready;
                }
            }
        }

        let mut newly_blocked = Vec::new();
        loop {
            let newly: Vec<usize> = self
                .states
                .iter()
                .enumerate()
                .filter_map(|(i, r)| {
                    if !matches!(r.state, State::Waiting { .. }) {
                        return None;
                    }
                    let bad = self.jobs[i].dependencies.iter().any(|d| {
                        let di = self.by_id[d];
                        matches!(self.states[di].state, State::Failed | State::Blocked)
                    });
                    bad.then_some(i)
                })
                .collect();

            if newly.is_empty() {
                break;
            }
            for i in newly {
                self.states[i].state = State::Blocked;
                newly_blocked.push(i);
            }
        }

        newly_blocked.sort_by(|&a, &b| self.jobs[a].id.cmp(&self.jobs[b].id));
        for i in newly_blocked {
            let dep = self.jobs[i]
                .dependencies
                .iter()
                .filter(|d| {
                    let di = self.by_id[*d];
                    matches!(self.states[di].state, State::Failed | State::Blocked)
                })
                .min()
                .cloned()
                .unwrap_or_default();
            lines.push(format!("{} skip {} {}", t, self.jobs[i].id, dep));
        }

        loop {
            let mut candidates: Vec<usize> = self
                .states
                .iter()
                .enumerate()
                .filter_map(|(i, r)| {
                    let ready = match r.state {
                        State::Waiting { ready_at, .. } => ready_at,
                        _ => return None,
                    };
                    if ready > t {
                        return None;
                    }
                    let ready_deps = self.jobs[i]
                        .dependencies
                        .iter()
                        .all(|d| matches!(self.states[self.by_id[d]].state, State::Succeeded));
                    ready_deps.then_some(i)
                })
                .collect();

            candidates.sort_by(|&a, &b| {
                self.effective_priority(a, t)
                    .cmp(&self.effective_priority(b, t))
                    .reverse()
                    .then_with(|| a.cmp(&b))
            });

            let mut action = None;
            for c in candidates {
                let running: Vec<usize> = self
                    .states
                    .iter()
                    .enumerate()
                    .filter_map(|(i, r)| matches!(r.state, State::Running { .. }).then_some(i))
                    .collect();

                let tag = self.jobs[c].tag.as_deref();
                let tag_free = tag.is_none()
                    || !running
                        .iter()
                        .any(|&i| self.jobs[i].tag.as_deref() == tag);

                if running.len() < self.capacity && tag_free {
                    action = Some((c, None));
                    break;
                }

                let mut victims: Vec<usize> = running
                    .into_iter()
                    .filter(|&v| {
                        self.jobs[c].priority >= self.jobs[v].priority.saturating_add(2)
                            && (self.jobs[c].tag.is_none()
                                || self.jobs[v].tag.as_deref() == self.jobs[c].tag.as_deref()
                                || !self.states.iter().enumerate().any(|(i, r)| {
                                    i != v
                                        && matches!(r.state, State::Running { .. })
                                        && self.jobs[i].tag.as_deref()
                                            == self.jobs[c].tag.as_deref()
                                }))
                    })
                    .collect();

                victims.sort_by(|&a, &b| {
                    self.jobs[a]
                        .priority
                        .cmp(&self.jobs[b].priority)
                        .then_with(|| {
                            self.states[b]
                                .started_at
                                .cmp(&self.states[a].started_at)
                        })
                        .then_with(|| self.jobs[b].id.cmp(&self.jobs[a].id))
                });

                if let Some(&v) = victims.first() {
                    action = Some((c, Some(v)));
                    break;
                }
            }

            let Some((c, victim)) = action else {
                break;
            };

            if let Some(v) = victim {
                let finish = match self.states[v].state {
                    State::Running { finish_at, .. } => finish_at,
                    _ => t,
                };
                let rem = finish.saturating_sub(t);
                lines.push(format!(
                    "{} preempt {} by {}",
                    t, self.jobs[v].id, self.jobs[c].id
                ));

                let attempt = match self.states[v].state {
                    State::Running { attempt, .. } => attempt,
                    _ => 0,
                };
                self.states[v].state = State::Waiting {
                    ready_at: t,
                    attempt,
                };
                self.states[v].ready_at = t;
                self.states[v].started_at = t;
                self.states[v].remaining = Some(rem);
            }

            let attempt = match self.states[c].state {
                State::Waiting { attempt, .. } => attempt,
                _ => 0,
            };
            let remaining = self.states[c]
                .remaining
                .take()
                .unwrap_or(self.jobs[c].duration);

            let next_attempt = if remaining == self.jobs[c].duration
                && !matches!(self.states[c].state, State::Waiting { attempt: 0, .. })
            {
                attempt.saturating_add(1)
            } else if remaining == self.jobs[c].duration && attempt == 0 {
                1
            } else {
                attempt
            };

            let finish = t.saturating_add(remaining);
            lines.push(format!(
                "{} start {} {} {}",
                t, self.jobs[c].id, next_attempt, remaining
            ));
            self.states[c].state = State::Running {
                attempt: next_attempt,
                finish_at: finish,
            };
            self.states[c].started_at = t;
        }
    }

    fn effective_priority(&self, i: usize, t: u64) -> u8 {
        if self.aging_ms == 0 {
            return self.jobs[i].priority;
        }
        let age = t.saturating_sub(self.states[i].ready_at) / self.aging_ms;
        (u64::from(self.jobs[i].priority) + age).min(9) as u8
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn job(id: &str, priority: u8, release_at: u64, duration: u64) -> Job {
        Job {
            id: id.into(),
            priority,
            dependencies: vec![],
            max_attempts: 2,
            release_at,
            duration,
            tag: None,
            failures: 0,
        }
    }

    #[test]
    fn completion_and_split_runs_are_recorded() {
        let mut p = Pool::new(vec![job("a", 1, 0, 5)], 1, 0, 3, 10).unwrap();
        assert_eq!(p.run(4).unwrap(), vec!["0 start a 1 5"]);
        assert_eq!(p.run(5).unwrap(), vec!["5 finish a 1 ok"]);
        assert_eq!(p.run(5).unwrap(), Vec::<String>::new());
        assert_eq!(p.status("a"), Ok(State::Succeeded));
    }

    #[test]
    fn retries_and_resumes_preserve_attempts_and_time() {
        let mut retry_job = job("a", 1, 0, 2);
        retry_job.failures = 1;
        let mut p = Pool::new(vec![retry_job], 1, 0, 3, 10).unwrap();
        assert_eq!(
            p.run(7).unwrap(),
            vec![
                "0 start a 1 2",
                "2 finish a 1 fail",
                "2 retry a 1 5",
                "5 start a 2 2",
                "7 finish a 2 ok",
            ]
        );

        let mut p = Pool::new(
            vec![job("low", 1, 0, 10), job("high", 8, 3, 2)],
            1,
            0,
            3,
            10,
        )
        .unwrap();
        assert_eq!(
            p.run(12).unwrap(),
            vec![
                "0 start low 1 10",
                "3 preempt low by high",
                "3 start high 1 2",
                "5 finish high 1 ok",
                "5 start low 1 7",
                "12 finish low 1 ok",
            ]
        );
    }
}

fn main() {}