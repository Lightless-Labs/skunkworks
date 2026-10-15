#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Job {
    pub id: String,
    pub ready_at: u64,
    pub priority: u8,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Dispatch {
    pub id: String,
    pub effective_priority: u8,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Error {
    TimeReversed,
    BadId,
    BadPriority,
    Duplicate,
    Unknown,
    Cancelled,
    Completed,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum JobState {
    Queued,
    Cancelled,
    Completed,
}

#[derive(Debug, Clone)]
struct StoredJob {
    job: Job,
    sequence: u64,
    state: JobState,
}

pub struct Scheduler {
    aging_ms: u64,
    last_time: u64,
    next_sequence: u64,
    jobs: Vec<StoredJob>,
}

impl Scheduler {
    pub fn new(aging_ms: u64) -> Self {
        Self {
            aging_ms,
            last_time: 0,
            next_sequence: 0,
            jobs: Vec::new(),
        }
    }

    pub fn insert(&mut self, now: u64, job: Job) -> Result<(), Error> {
        if now < self.last_time {
            return Err(Error::TimeReversed);
        }
        if !Self::valid_id(&job.id) {
            return Err(Error::BadId);
        }
        if job.priority > 9 {
            return Err(Error::BadPriority);
        }
        if self.jobs.iter().any(|stored| stored.job.id == job.id) {
            return Err(Error::Duplicate);
        }

        let sequence = self.next_sequence;
        self.next_sequence = self.next_sequence.wrapping_add(1);
        self.jobs.push(StoredJob {
            job,
            sequence,
            state: JobState::Queued,
        });
        self.last_time = now;
        Ok(())
    }

    pub fn cancel(&mut self, now: u64, id: &str) -> Result<(), Error> {
        if now < self.last_time {
            return Err(Error::TimeReversed);
        }

        let Some(index) = self.jobs.iter().position(|stored| stored.job.id == id) else {
            return Err(Error::Unknown);
        };

        match self.jobs[index].state {
            JobState::Queued => {
                self.jobs[index].state = JobState::Cancelled;
                self.last_time = now;
                Ok(())
            }
            JobState::Cancelled => Err(Error::Cancelled),
            JobState::Completed => Err(Error::Completed),
        }
    }

    pub fn next_ready(&mut self, now: u64) -> Result<Option<Dispatch>, Error> {
        if now < self.last_time {
            return Err(Error::TimeReversed);
        }

        let mut selected: Option<(usize, u8, u64)> = None;
        for (index, stored) in self.jobs.iter().enumerate() {
            if stored.state != JobState::Queued || stored.job.ready_at > now {
                continue;
            }

            let effective_priority = self.effective_priority(&stored.job, now);
            match selected {
                None => selected = Some((index, effective_priority, stored.sequence)),
                Some((_, best_priority, best_sequence))
                    if effective_priority > best_priority
                        || (effective_priority == best_priority
                            && stored.sequence < best_sequence) =>
                {
                    selected = Some((index, effective_priority, stored.sequence));
                }
                _ => {}
            }
        }

        self.last_time = now;
        if let Some((index, effective_priority, _)) = selected {
            self.jobs[index].state = JobState::Completed;
            Ok(Some(Dispatch {
                id: self.jobs[index].job.id.clone(),
                effective_priority,
            }))
        } else {
            Ok(None)
        }
    }

    fn valid_id(id: &str) -> bool {
        let bytes = id.as_bytes();
        (1..=16).contains(&bytes.len())
            && bytes
                .iter()
                .all(|&byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'-')
    }

    fn effective_priority(&self, job: &Job, now: u64) -> u8 {
        if self.aging_ms == 0 {
            return job.priority;
        }

        let age_steps = (now - job.ready_at) / self.aging_ms;
        let remaining = 9 - job.priority;
        if age_steps >= u64::from(remaining) {
            9
        } else {
            job.priority + age_steps as u8
        }
    }
}

fn main() {}