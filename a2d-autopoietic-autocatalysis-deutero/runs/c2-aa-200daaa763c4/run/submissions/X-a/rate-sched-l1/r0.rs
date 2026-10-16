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

#[derive(Debug, Clone, PartialEq, Eq)]
enum Status {
    Queued,
    Cancelled,
    Completed,
}

struct Entry {
    job: Job,
    sequence: u64,
    status: Status,
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
        if now < self.last_time {
            return Err(Error::TimeReversed);
        }

        let id_bytes = job.id.as_bytes();
        if id_bytes.is_empty()
            || id_bytes.len() > 16
            || !id_bytes
                .iter()
                .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || *b == b'-')
        {
            return Err(Error::BadId);
        }

        if job.priority > 9 {
            return Err(Error::BadPriority);
        }

        if self.entries.iter().any(|entry| entry.job.id == job.id) {
            return Err(Error::Duplicate);
        }

        let sequence = self.next_sequence;
        self.next_sequence = self.next_sequence.wrapping_add(1);
        self.entries.push(Entry {
            job,
            sequence,
            status: Status::Queued,
        });
        self.last_time = now;
        Ok(())
    }

    pub fn cancel(&mut self, now: u64, id: &str) -> Result<(), Error> {
        if now < self.last_time {
            return Err(Error::TimeReversed);
        }

        let Some(entry) = self.entries.iter_mut().find(|entry| entry.job.id == id) else {
            return Err(Error::Unknown);
        };

        match entry.status {
            Status::Queued => entry.status = Status::Cancelled,
            Status::Cancelled => return Err(Error::Cancelled),
            Status::Completed => return Err(Error::Completed),
        }

        self.last_time = now;
        Ok(())
    }

    pub fn next_ready(&mut self, now: u64) -> Result<Option<Dispatch>, Error> {
        if now < self.last_time {
            return Err(Error::TimeReversed);
        }

        let mut selected: Option<(usize, u8, u64)> = None;

        for (index, entry) in self.entries.iter().enumerate() {
            if entry.status != Status::Queued || entry.job.ready_at > now {
                continue;
            }

            let effective_priority = if self.aging_ms == 0 {
                entry.job.priority
            } else {
                let age_steps = (now - entry.job.ready_at) / self.aging_ms;
                let boosted = u64::from(entry.job.priority).saturating_add(age_steps);
                boosted.min(9) as u8
            };

            let replace = match selected {
                None => true,
                Some((_, best_priority, best_sequence)) => {
                    effective_priority > best_priority
                        || (effective_priority == best_priority && entry.sequence < best_sequence)
                }
            };

            if replace {
                selected = Some((index, effective_priority, entry.sequence));
            }
        }

        self.last_time = now;

        let Some((index, effective_priority, _)) = selected else {
            return Ok(None);
        };

        self.entries[index].status = Status::Completed;
        Ok(Some(Dispatch {
            id: self.entries[index].job.id.clone(),
            effective_priority,
        }))
    }
}