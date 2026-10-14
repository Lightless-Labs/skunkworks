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
        self.entries.push(Entry {
            job,
            sequence,
            status: Status::Queued,
        });
        self.next_sequence = self.next_sequence.wrapping_add(1);
        self.last_time = now;
        Ok(())
    }

    pub fn cancel(&mut self, now: u64, id: &str) -> Result<(), Error> {
        self.check_time(now)?;

        let Some(index) = self.entries.iter().position(|entry| entry.job.id == id) else {
            return Err(Error::Unknown);
        };

        match self.entries[index].status {
            Status::Queued => self.entries[index].status = Status::Cancelled,
            Status::Cancelled => return Err(Error::Cancelled),
            Status::Completed => return Err(Error::Completed),
        }

        self.last_time = now;
        Ok(())
    }

    pub fn next_ready(&mut self, now: u64) -> Result<Option<Dispatch>, Error> {
        self.check_time(now)?;

        let mut best: Option<(usize, u8, u64)> = None;
        for (index, entry) in self.entries.iter().enumerate() {
            if entry.status != Status::Queued || entry.job.ready_at > now {
                continue;
            }

            let effective = self.effective_priority(&entry.job, now);
            match best {
                None => best = Some((index, effective, entry.sequence)),
                Some((_, best_priority, best_sequence))
                    if effective > best_priority
                        || (effective == best_priority && entry.sequence < best_sequence) =>
                {
                    best = Some((index, effective, entry.sequence));
                }
                _ => {}
            }
        }

        self.last_time = now;
        let Some((index, effective_priority, _)) = best else {
            return Ok(None);
        };

        self.entries[index].status = Status::Completed;
        Ok(Some(Dispatch {
            id: self.entries[index].job.id.clone(),
            effective_priority,
        }))
    }

    fn check_time(&self, now: u64) -> Result<(), Error> {
        if now < self.last_time {
            Err(Error::TimeReversed)
        } else {
            Ok(())
        }
    }

    fn valid_id(id: &str) -> bool {
        (1..=16).contains(&id.len())
            && id
                .bytes()
                .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'-')
    }

    fn effective_priority(&self, job: &Job, now: u64) -> u8 {
        if self.aging_ms == 0 {
            return job.priority;
        }

        let age_steps = (now - job.ready_at) / self.aging_ms;
        let remaining = 9 - job.priority as u64;
        if age_steps >= remaining {
            9
        } else {
            job.priority + age_steps as u8
        }
    }
}