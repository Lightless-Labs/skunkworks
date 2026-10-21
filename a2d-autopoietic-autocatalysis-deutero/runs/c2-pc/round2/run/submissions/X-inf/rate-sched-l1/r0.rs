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

#[derive(Debug, Clone, PartialEq, Eq)]
struct Entry {
    job: Job,
    sequence: u128,
    status: Status,
}

pub struct Scheduler {
    aging_ms: u64,
    last_time: u64,
    next_sequence: u128,
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

        if !Self::valid_id(&job.id) {
            return Err(Error::BadId);
        }
        if job.priority > 9 {
            return Err(Error::BadPriority);
        }
        if self.entries.iter().any(|entry| entry.job.id == job.id) {
            return Err(Error::Duplicate);
        }

        let sequence = self.next_sequence;
        let next_sequence = sequence.checked_add(1).expect("sequence overflow");
        self.entries.push(Entry {
            job,
            sequence,
            status: Status::Queued,
        });
        self.next_sequence = next_sequence;
        self.last_time = now;
        Ok(())
    }

    pub fn cancel(&mut self, now: u64, id: &str) -> Result<(), Error> {
        self.check_time(now)?;

        let entry = self
            .entries
            .iter_mut()
            .find(|entry| entry.job.id == id)
            .ok_or(Error::Unknown)?;

        match entry.status {
            Status::Queued => entry.status = Status::Cancelled,
            Status::Cancelled => return Err(Error::Cancelled),
            Status::Completed => return Err(Error::Completed),
        }

        self.last_time = now;
        Ok(())
    }

    pub fn next_ready(&mut self, now: u64) -> Result<Option<Dispatch>, Error> {
        self.check_time(now)?;

        let mut selected: Option<(usize, u8, u128)> = None;
        for (index, entry) in self.entries.iter().enumerate() {
            if entry.status != Status::Queued || entry.job.ready_at > now {
                continue;
            }

            let effective_priority = if self.aging_ms == 0 {
                entry.job.priority
            } else {
                let age_steps = (now - entry.job.ready_at) / self.aging_ms;
                let remaining = 9u64 - u64::from(entry.job.priority);
                if age_steps >= remaining {
                    9
                } else {
                    entry.job.priority + age_steps as u8
                }
            };

            let should_select = match selected {
                None => true,
                Some((_, best_priority, best_sequence)) => {
                    effective_priority > best_priority
                        || (effective_priority == best_priority && entry.sequence < best_sequence)
                }
            };

            if should_select {
                selected = Some((index, effective_priority, entry.sequence));
            }
        }

        self.last_time = now;
        if let Some((index, effective_priority, _)) = selected {
            let entry = &mut self.entries[index];
            entry.status = Status::Completed;
            Ok(Some(Dispatch {
                id: entry.job.id.clone(),
                effective_priority,
            }))
        } else {
            Ok(None)
        }
    }

    fn check_time(&self, now: u64) -> Result<(), Error> {
        if now < self.last_time {
            Err(Error::TimeReversed)
        } else {
            Ok(())
        }
    }

    fn valid_id(id: &str) -> bool {
        let bytes = id.as_bytes();
        (1..=16).contains(&bytes.len())
            && bytes
                .iter()
                .all(|&byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'-')
    }
}