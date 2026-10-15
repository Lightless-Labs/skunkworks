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

#[derive(Debug, Clone)]
struct Entry {
    job: Job,
    sequence: u64,
    status: Status,
}

#[derive(Debug, Clone)]
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

        let id_is_valid = (1..=16).contains(&job.id.len())
            && job
                .id
                .bytes()
                .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-');
        if !id_is_valid {
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
            status: Status::Queued,
        });
        self.next_sequence += 1;
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
                let remaining = 9 - entry.job.priority as u64;
                if age_steps >= remaining {
                    9
                } else {
                    entry.job.priority + age_steps as u8
                }
            };

            match selected {
                None => selected = Some((index, effective_priority, entry.sequence)),
                Some((_, best_priority, best_sequence))
                    if effective_priority > best_priority
                        || (effective_priority == best_priority
                            && entry.sequence < best_sequence) =>
                {
                    selected = Some((index, effective_priority, entry.sequence));
                }
                _ => {}
            }
        }

        let result = if let Some((index, effective_priority, _)) = selected {
            self.entries[index].status = Status::Completed;
            Some(Dispatch {
                id: self.entries[index].job.id.clone(),
                effective_priority,
            })
        } else {
            None
        };

        self.last_time = now;
        Ok(result)
    }
}

fn main() {}