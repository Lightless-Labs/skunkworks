use std::collections::{HashMap, HashSet};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Job {
    pub id: String,
    pub priority: u8,
    pub dependencies: Vec<String>,
    pub max_attempts: u8,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum State {
    Waiting { ready_at: u64 },
    Running { attempt: u8 },
    Succeeded,
    Failed,
    Blocked,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Ticket {
    pub id: String,
    pub attempt: u8,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Error {
    Config,
    BadId(usize),
    BadPriority(usize),
    BadAttempts(usize),
    Duplicate(usize),
    DuplicateDependency(usize, usize),
    MissingDependency(usize, usize),
    Cycle,
    TimeReversed,
    Busy,
    NoCycle,
    Unknown,
    NotRunning,
}

struct RuntimeJob {
    state: State,
    attempts: u8,
}

pub struct Runner {
    jobs: Vec<Job>,
    dependencies: Vec<Vec<usize>>,
    index_by_id: HashMap<String, usize>,
    runtime: Vec<RuntimeJob>,
    aging_ms: u64,
    retry_base: u64,
    retry_cap: u64,
    last_time: Option<u64>,
    cycle_started: bool,
}

impl Runner {
    /// Validates and constructs a runner with a fixed job dependency graph.
    pub fn new(
        jobs: Vec<Job>,
        aging_ms: u64,
        retry_base: u64,
        retry_cap: u64,
    ) -> Result<Self, Error> {
        if retry_base == 0 || retry_cap < retry_base {
            return Err(Error::Config);
        }

        let mut index_by_id = HashMap::new();
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
            if index_by_id.insert(job.id.clone(), i).is_some() {
                return Err(Error::Duplicate(i));
            }
        }

        let mut dependencies = Vec::with_capacity(jobs.len());
        for (job_index, job) in jobs.iter().enumerate() {
            let mut seen = HashSet::new();
            let mut resolved = Vec::with_capacity(job.dependencies.len());
            for (dependency_index, dependency) in job.dependencies.iter().enumerate() {
                if !seen.insert(dependency) {
                    return Err(Error::DuplicateDependency(job_index, dependency_index));
                }
                let Some(&target) = index_by_id.get(dependency) else {
                    return Err(Error::MissingDependency(job_index, dependency_index));
                };
                resolved.push(target);
            }
            dependencies.push(resolved);
        }

        if has_cycle(&dependencies) {
            return Err(Error::Cycle);
        }

        let runtime = jobs
            .iter()
            .map(|_| RuntimeJob {
                state: State::Waiting { ready_at: 0 },
                attempts: 0,
            })
            .collect();

        Ok(Self {
            jobs,
            dependencies,
            index_by_id,
            runtime,
            aging_ms,
            retry_base,
            retry_cap,
            last_time: None,
            cycle_started: false,
        })
    }

    /// Starts a fresh cycle if the previous cycle has no waiting or running jobs.
    pub fn begin_cycle(&mut self, now: u64) -> Result<(), Error> {
        self.check_time(now)?;
        if self.cycle_started
            && self.runtime.iter().any(|job| {
                matches!(job.state, State::Waiting { .. } | State::Running { .. })
            })
        {
            return Err(Error::Busy);
        }

        for job in &mut self.runtime {
            job.state = State::Waiting { ready_at: now };
            job.attempts = 0;
        }
        self.cycle_started = true;
        self.last_time = Some(now);
        Ok(())
    }

    /// Dispatches the highest-priority eligible job, if one is ready.
    pub fn next_ready(&mut self, now: u64) -> Result<Option<Ticket>, Error> {
        self.check_time(now)?;
        if !self.cycle_started {
            return Err(Error::NoCycle);
        }

        let mut best: Option<(usize, u8)> = None;
        for i in 0..self.jobs.len() {
            let State::Waiting { ready_at } = self.runtime[i].state else {
                continue;
            };
            if ready_at > now
                || !self.dependencies[i]
                    .iter()
                    .all(|&dep| self.runtime[dep].state == State::Succeeded)
            {
                continue;
            }

            let effective = if self.aging_ms == 0 {
                self.jobs[i].priority
            } else {
                let age = (now - ready_at) / self.aging_ms;
                (self.jobs[i].priority as u64 + age).min(9) as u8
            };
            if best.is_none_or(|(_, priority)| effective > priority) {
                best = Some((i, effective));
            }
        }

        self.last_time = Some(now);
        let Some((i, _)) = best else {
            return Ok(None);
        };

        self.runtime[i].attempts += 1;
        let attempt = self.runtime[i].attempts;
        self.runtime[i].state = State::Running { attempt };
        Ok(Some(Ticket {
            id: self.jobs[i].id.clone(),
            attempt,
        }))
    }

    /// Completes a running job and applies retry or dependency-blocking rules.
    pub fn complete(&mut self, now: u64, id: &str, success: bool) -> Result<(), Error> {
        self.check_time(now)?;
        if !self.cycle_started {
            return Err(Error::NoCycle);
        }
        let Some(&i) = self.index_by_id.get(id) else {
            return Err(Error::Unknown);
        };
        if !matches!(self.runtime[i].state, State::Running { .. }) {
            return Err(Error::NotRunning);
        }

        let attempt = self.runtime[i].attempts;
        if success {
            self.runtime[i].state = State::Succeeded;
        } else if attempt == self.jobs[i].max_attempts {
            self.runtime[i].state = State::Failed;
            self.propagate_blocked();
        } else {
            let delay = self.retry_delay(attempt);
            self.runtime[i].state = State::Waiting {
                ready_at: now.saturating_add(delay),
            };
        }

        self.last_time = Some(now);
        Ok(())
    }

    /// Returns an owned snapshot of a job's current-cycle state.
    pub fn status(&self, id: &str) -> Result<State, Error> {
        if !self.cycle_started {
            return Err(Error::NoCycle);
        }
        let Some(&i) = self.index_by_id.get(id) else {
            return Err(Error::Unknown);
        };
        Ok(self.runtime[i].state.clone())
    }

    fn check_time(&self, now: u64) -> Result<(), Error> {
        if self.last_time.is_some_and(|last| now < last) {
            Err(Error::TimeReversed)
        } else {
            Ok(())
        }
    }

    fn retry_delay(&self, attempt: u8) -> u64 {
        let mut delay = self.retry_base;
        for _ in 1..attempt {
            if delay >= self.retry_cap || delay > self.retry_cap / 2 {
                return self.retry_cap;
            }
            delay *= 2;
        }
        delay.min(self.retry_cap)
    }

    fn propagate_blocked(&mut self) {
        loop {
            let mut newly_blocked = Vec::new();
            for i in 0..self.jobs.len() {
                if matches!(self.runtime[i].state, State::Waiting { .. })
                    && self.dependencies[i].iter().any(|&dep| {
                        matches!(
                            self.runtime[dep].state,
                            State::Failed | State::Blocked
                        )
                    })
                {
                    newly_blocked.push(i);
                }
            }
            if newly_blocked.is_empty() {
                break;
            }
            for i in newly_blocked {
                self.runtime[i].state = State::Blocked;
            }
        }
    }
}

fn valid_id(id: &str) -> bool {
    (1..=16).contains(&id.len())
        && id
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
    (0..dependencies.len()).any(|i| visit(i, dependencies, &mut marks))
}

fn main() {}

#[cfg(test)]
mod tests {
    use super::*;

    fn job(id: &str, priority: u8, dependencies: &[&str], max_attempts: u8) -> Job {
        Job {
            id: id.into(),
            priority,
            dependencies: dependencies.iter().map(|s| (*s).into()).collect(),
            max_attempts,
        }
    }

    #[test]
    fn dispatch_completion_and_new_cycle() {
        let mut runner = Runner::new(vec![job("task", 3, &[], 1)], 0, 5, 20).unwrap();
        runner.begin_cycle(0).unwrap();
        let ticket = runner.next_ready(0).unwrap().unwrap();
        assert_eq!(ticket, Ticket { id: "task".into(), attempt: 1 });
        runner.complete(0, "task", true).unwrap();
        assert_eq!(runner.status("task"), Ok(State::Succeeded));
        runner.begin_cycle(0).unwrap();
        assert_eq!(runner.status("task"), Ok(State::Waiting { ready_at: 0 }));
    }

    #[test]
    fn invalid_configuration_and_dependencies_are_rejected() {
        assert_eq!(Runner::new(vec![], 0, 0, 1).err(), Some(Error::Config));
        assert_eq!(
            Runner::new(vec![job("x", 1, &["missing"], 1)], 0, 1, 1).err(),
            Some(Error::MissingDependency(0, 0))
        );
        assert_eq!(
            Runner::new(vec![job("x", 1, &["x"], 1)], 0, 1, 1).err(),
            Some(Error::Cycle)
        );
    }

    #[test]
    fn dependency_retry_and_blocking_end_to_end() {
        let mut runner = Runner::new(
            vec![
                job("child", 9, &["root"], 2),
                job("root", 1, &[], 2),
                job("other", 0, &[], 1),
            ],
            0,
            5,
            20,
        )
        .unwrap();
        runner.begin_cycle(0).unwrap();
        assert_eq!(runner.next_ready(0).unwrap().unwrap().id, "root");
        runner.complete(0, "root", false).unwrap();
        assert_eq!(runner.next_ready(0).unwrap().unwrap().id, "other");
        runner.complete(0, "other", false).unwrap();
        assert_eq!(runner.status("child"), Ok(State::Waiting { ready_at: 0 }));
        assert_eq!(runner.status("other"), Ok(State::Failed));
        assert_eq!(runner.next_ready(5).unwrap().unwrap().id, "root");
        runner.complete(5, "root", true).unwrap();
        assert_eq!(runner.next_ready(5).unwrap().unwrap().id, "child");
        runner.complete(5, "child", true).unwrap();
        assert_eq!(runner.status("child"), Ok(State::Succeeded));
    }

    #[test]
    fn failed_completion_blocks_dependents_transitively() {
        let mut runner = Runner::new(
            vec![
                job("root", 1, &[], 1),
                job("middle", 1, &["root"], 1),
                job("leaf", 1, &["middle"], 1),
            ],
            0,
            1,
            4,
        )
        .unwrap();
        runner.begin_cycle(0).unwrap();
        runner.next_ready(0).unwrap();
        runner.complete(0, "root", false).unwrap();
        assert_eq!(runner.status("middle"), Ok(State::Blocked));
        assert_eq!(runner.status("leaf"), Ok(State::Blocked));
    }

    #[test]
    fn multiple_dispatches_preserve_in_flight_jobs() {
        let mut runner = Runner::new(
            vec![job("first", 2, &[], 1), job("second", 2, &[], 1)],
            0,
            1,
            2,
        )
        .unwrap();
        runner.begin_cycle(0).unwrap();
        assert_eq!(runner.next_ready(0).unwrap().unwrap().id, "first");
        assert_eq!(runner.next_ready(0).unwrap().unwrap().id, "second");
        assert_eq!(runner.next_ready(0), Ok(None));
        runner.complete(0, "first", true).unwrap();
        runner.complete(0, "second", true).unwrap();
        runner.begin_cycle(0).unwrap();
    }
}