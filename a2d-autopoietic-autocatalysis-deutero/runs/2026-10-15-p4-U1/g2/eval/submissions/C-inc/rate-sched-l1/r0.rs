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

#[derive(Clone, Copy, PartialEq, Eq)]
enum JobState {
    Queued,
    Cancelled,
    Completed,
}

struct Entry {
    job: Job,
    sequence: u64,
    state: JobState,
}

pub struct Scheduler {
    aging_ms: u64,
    last_time: u64,
    next_sequence: u64,
    entries: Vec<Entry>,
}

impl Scheduler {
    pub fn new(aging_ms: u64) -> Self {
        Self {
            aging_ms,
            last_time: 0,
            next_sequence: 0,
            entries: Vec::new(),
        }
    }

    pub fn insert(&mut self, now: u64, job: Job) -> Result<(), Error> {
        self.check_time(now)?;

        if !valid_id(&job.id) {
            return Err(Error::BadId);
        }
        if job.priority > 9 {
            return Err(Error::BadPriority);
        }
        if self.entries.iter().any(|entry| entry.job.id == job.id) {
            return Err(Error::Duplicate);
        }

        self.entries.push(Entry {
            job,
            sequence: self.next_sequence,
            state: JobState::Queued,
        });
        self.next_sequence += 1;
        self.last_time = now;
        Ok(())
    }

    pub fn cancel(&mut self, now: u64, id: &str) -> Result<(), Error> {
        self.check_time(now)?;

        let entry = match self.entries.iter_mut().find(|entry| entry.job.id == id) {
            Some(entry) => entry,
            None => return Err(Error::Unknown),
        };

        match entry.state {
            JobState::Queued => entry.state = JobState::Cancelled,
            JobState::Cancelled => return Err(Error::Cancelled),
            JobState::Completed => return Err(Error::Completed),
        }

        self.last_time = now;
        Ok(())
    }

    pub fn next_ready(&mut self, now: u64) -> Result<Option<Dispatch>, Error> {
        self.check_time(now)?;

        let mut selected: Option<(usize, u8, u64)> = None;

        for (index, entry) in self.entries.iter().enumerate() {
            if entry.state != JobState::Queued || entry.job.ready_at > now {
                continue;
            }

            let effective_priority = if self.aging_ms == 0 {
                entry.job.priority
            } else {
                let age_steps = (now - entry.job.ready_at) / self.aging_ms;
                (entry.job.priority as u64 + age_steps).min(9) as u8
            };

            let is_better = match selected {
                None => true,
                Some((_, best_priority, best_sequence)) => {
                    effective_priority > best_priority
                        || (effective_priority == best_priority && entry.sequence < best_sequence)
                }
            };

            if is_better {
                selected = Some((index, effective_priority, entry.sequence));
            }
        }

        self.last_time = now;

        match selected {
            None => Ok(None),
            Some((index, effective_priority, _)) => {
                let entry = &mut self.entries[index];
                entry.state = JobState::Completed;
                Ok(Some(Dispatch {
                    id: entry.job.id.clone(),
                    effective_priority,
                }))
            }
        }
    }

    fn check_time(&self, now: u64) -> Result<(), Error> {
        if now < self.last_time {
            Err(Error::TimeReversed)
        } else {
            Ok(())
        }
    }
}

fn valid_id(id: &str) -> bool {
    let bytes = id.as_bytes();
    (1..=16).contains(&bytes.len())
        && bytes
            .iter()
            .all(|&byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'-')
}

fn main() {}