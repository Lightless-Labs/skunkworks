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

        if !Self::valid_id(&job.id) {
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

        let selected = self
            .entries
            .iter()
            .enumerate()
            .filter(|(_, entry)| {
                entry.status == Status::Queued && entry.job.ready_at <= now
            })
            .map(|(index, entry)| {
                let effective_priority = if self.aging_ms == 0 {
                    entry.job.priority
                } else {
                    let waited = now - entry.job.ready_at;
                    let gained = waited / self.aging_ms;
                    (u64::from(entry.job.priority) + gained).min(9) as u8
                };
                (index, entry.sequence, effective_priority)
            })
            .max_by(|a, b| {
                a.2.cmp(&b.2)
                    .then_with(|| b.1.cmp(&a.1))
            });

        self.last_time = now;

        let Some((index, _, effective_priority)) = selected else {
            return Ok(None);
        };

        self.entries[index].status = Status::Completed;
        Ok(Some(Dispatch {
            id: self.entries[index].job.id.clone(),
            effective_priority,
        }))
    }

    fn valid_id(id: &str) -> bool {
        let bytes = id.as_bytes();
        (1..=16).contains(&bytes.len())
            && bytes
                .iter()
                .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || *byte == b'-')
    }
}

fn main() {}