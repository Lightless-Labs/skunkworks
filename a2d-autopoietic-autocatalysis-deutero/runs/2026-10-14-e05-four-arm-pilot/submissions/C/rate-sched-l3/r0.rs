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

struct JobState {
    state: State,
    attempts: u8,
}

pub struct Runner {
    jobs: Vec<Job>,
    dependencies: Vec<Vec<usize>>,
    states: Vec<JobState>,
    aging_ms: u64,
    retry_base: u64,
    retry_cap: u64,
    last_time: Option<u64>,
    cycle_started: bool,
}

impl Runner {
    /// Creates a runner after validating jobs, dependencies, and retry settings.
    pub fn new(
        jobs: Vec<Job>,
        aging_ms: u64,
        retry_base: u64,
        retry_cap: u64,
    ) -> Result<Self, Error> {
        if retry_base == 0 || retry_cap < retry_base {
            return Err(Error::Config);
        }

        let mut ids = HashMap::new();
        for (index, job) in jobs.iter().enumerate() {
            let valid_id = !job.id.is_empty()
                && job.id.len() <= 16
                && job
                    .id
                    .bytes()
                    .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-');
            if !valid_id {
                return Err(Error::BadId(index));
            }
            if job.priority > 9 {
                return Err(Error::BadPriority(index));
            }
            if !(1..=32).contains(&job.max_attempts) {
                return Err(Error::BadAttempts(index));
            }
            if ids.insert(job.id.clone(), index).is_some() {
                return Err(Error::Duplicate(index));
            }
        }

        let mut dependencies = Vec::with_capacity(jobs.len());
        for (job_index, job) in jobs.iter().enumerate() {
            let mut seen = HashSet::new();
            let mut resolved = Vec::with_capacity(job.dependencies.len());
            for (dep_index, name) in job.dependencies.iter().enumerate() {
                if !seen.insert(name.as_str()) {
                    return Err(Error::DuplicateDependency(job_index, dep_index));
                }
                let Some(&dep) = ids.get(name) else {
                    return Err(Error::MissingDependency(job_index, dep_index));
                };
                resolved.push(dep);
            }
            dependencies.push(resolved);
        }

        fn visit(
            node: usize,
            dependencies: &[Vec<usize>],
            marks: &mut [u8],
        ) -> bool {
            if marks[node] == 1 {
                return false;
            }
            if marks[node] == 2 {
                return true;
            }
            marks[node] = 1;
            for &dep in &dependencies[node] {
                if !visit(dep, dependencies, marks) {
                    return false;
                }
            }
            marks[node] = 2;
            true
        }

        let mut marks = vec![0; jobs.len()];
        for node in 0..jobs.len() {
            if !visit(node, &dependencies, &mut marks) {
                return Err(Error::Cycle);
            }
        }

        let states = jobs
            .iter()
            .map(|_| JobState {
                state: State::Waiting { ready_at: 0 },
                attempts: 0,
            })
            .collect();

        Ok(Self {
            jobs,
            dependencies,
            states,
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
            && self.states.iter().any(|entry| {
                matches!(entry.state, State::Waiting { .. } | State::Running { .. })
            })
        {
            return Err(Error::Busy);
        }

        for entry in &mut self.states {
            entry.state = State::Waiting { ready_at: now };
            entry.attempts = 0;
        }
        self.cycle_started = true;
        self.last_time = Some(now);
        Ok(())
    }

    /// Dispatches the eligible waiting job with the highest effective priority.
    pub fn next_ready(&mut self, now: u64) -> Result<Option<Ticket>, Error> {
        self.check_time(now)?;
        if !self.cycle_started {
            return Err(Error::NoCycle);
        }

        let mut selected: Option<(usize, u8)> = None;
        for (index, job) in self.jobs.iter().enumerate() {
            let State::Waiting { ready_at } = self.states[index].state else {
                continue;
            };
            if ready_at > now
                || self.dependencies[index]
                    .iter()
                    .any(|&dep| self.states[dep].state != State::Succeeded)
            {
                continue;
            }

            let effective = if self.aging_ms == 0 {
                job.priority
            } else {
                let gained = (now - ready_at) / self.aging_ms;
                (u64::from(job.priority) + gained).min(9) as u8
            };
            if selected.is_none_or(|(_, best)| effective > best) {
                selected = Some((index, effective));
            }
        }

        let result = if let Some((index, _)) = selected {
            self.states[index].attempts += 1;
            let attempt = self.states[index].attempts;
            self.states[index].state = State::Running { attempt };
            Some(Ticket {
                id: self.jobs[index].id.clone(),
                attempt,
            })
        } else {
            None
        };

        self.last_time = Some(now);
        Ok(result)
    }

    /// Records a running job's completion and propagates terminal dependency failures.
    pub fn complete(&mut self, now: u64, id: &str, success: bool) -> Result<(), Error> {
        self.check_time(now)?;
        if !self.cycle_started {
            return Err(Error::NoCycle);
        }
        let Some(index) = self.jobs.iter().position(|job| job.id == id) else {
            return Err(Error::Unknown);
        };
        let State::Running { attempt } = self.states[index].state else {
            return Err(Error::NotRunning);
        };

        if success {
            self.states[index].state = State::Succeeded;
        } else if attempt == self.jobs[index].max_attempts {
            self.states[index].state = State::Failed;
            self.propagate_blocked();
        } else {
            let shift = u32::from(attempt.saturating_sub(1));
            let delay = self
                .retry_base
                .checked_shl(shift)
                .unwrap_or(u64::MAX)
                .min(self.retry_cap);
            self.states[index].state = State::Waiting {
                ready_at: now.saturating_add(delay),
            };
        }

        self.last_time = Some(now);
        Ok(())
    }

    /// Returns an owned snapshot of a job's state in the current cycle.
    pub fn status(&self, id: &str) -> Result<State, Error> {
        if !self.cycle_started {
            return Err(Error::NoCycle);
        }
        let Some(index) = self.jobs.iter().position(|job| job.id == id) else {
            return Err(Error::Unknown);
        };
        Ok(self.states[index].state.clone())
    }

    fn check_time(&self, now: u64) -> Result<(), Error> {
        if self.last_time.is_some_and(|last| now < last) {
            Err(Error::TimeReversed)
        } else {
            Ok(())
        }
    }

    fn propagate_blocked(&mut self) {
        loop {
            let to_block: Vec<usize> = self
                .states
                .iter()
                .enumerate()
                .filter_map(|(index, entry)| {
                    if !matches!(entry.state, State::Waiting { .. }) {
                        return None;
                    }
                    self.dependencies[index]
                        .iter()
                        .any(|&dep| {
                            matches!(self.states[dep].state, State::Failed | State::Blocked)
                        })
                        .then_some(index)
                })
                .collect();

            if to_block.is_empty() {
                break;
            }
            for index in to_block {
                self.states[index].state = State::Blocked;
            }
        }
    }
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
    fn dispatch_and_complete_cycle() {
        let mut runner = Runner::new(vec![job("a", 1, &[], 1)], 0, 5, 20).unwrap();
        runner.begin_cycle(0).unwrap();
        assert_eq!(
            runner.next_ready(0).unwrap(),
            Some(Ticket {
                id: "a".into(),
                attempt: 1
            })
        );
        runner.complete(0, "a", true).unwrap();
        assert_eq!(runner.status("a"), Ok(State::Succeeded));
        runner.begin_cycle(1).unwrap();
    }

    #[test]
    fn invalid_configuration_and_job_are_reported() {
        assert_eq!(Runner::new(vec![], 0, 0, 1).err(), Some(Error::Config));
        assert_eq!(
            Runner::new(vec![job("Bad", 0, &[], 1)], 0, 1, 1).err(),
            Some(Error::BadId(0))
        );
    }

    #[test]
    fn dependencies_block_until_success_and_failures_propagate() {
        let mut runner = Runner::new(
            vec![job("child", 9, &["root"], 1), job("root", 1, &[], 1)],
            0,
            5,
            20,
        )
        .unwrap();
        runner.begin_cycle(0).unwrap();
        assert_eq!(runner.next_ready(0).unwrap().unwrap().id, "root");
        runner.complete(0, "root", false).unwrap();
        assert_eq!(runner.status("root"), Ok(State::Failed));
        assert_eq!(runner.status("child"), Ok(State::Blocked));
    }

    #[test]
    fn retry_waits_for_backoff() {
        let mut runner = Runner::new(vec![job("a", 1, &[], 2)], 0, 5, 20).unwrap();
        runner.begin_cycle(0).unwrap();
        runner.next_ready(0).unwrap();
        runner.complete(1, "a", false).unwrap();
        assert_eq!(runner.status("a"), Ok(State::Waiting { ready_at: 6 }));
        assert_eq!(runner.next_ready(5), Ok(None));
        assert_eq!(runner.next_ready(6).unwrap().unwrap().attempt, 2);
    }
}