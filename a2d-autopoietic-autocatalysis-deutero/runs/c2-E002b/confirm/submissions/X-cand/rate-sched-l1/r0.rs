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
enum JobState {
    Queued,
    Cancelled,
    Completed,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct Entry {
    job: Job,
    sequence: u128,
    state: JobState,
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

        if !valid_id(&job.id) {
            return Err(Error::BadId);
        }
        if job.priority > 9 {
            return Err(Error::BadPriority);
        }
        if self.entries.iter().any(|entry| entry.job.id == job.id) {
            return Err(Error::Duplicate);
        }

        let sequence = self.next_sequence;
        self.next_sequence = self.next_sequence.checked_add(1).expect("sequence overflow");
        self.entries.push(Entry {
            job,
            sequence,
            state: JobState::Queued,
        });
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

        let selected = self
            .entries
            .iter()
            .enumerate()
            .filter(|(_, entry)| {
                entry.state == JobState::Queued && entry.job.ready_at <= now
            })
            .map(|(index, entry)| {
                (
                    index,
                    effective_priority(&entry.job, now, self.aging_ms),
                    entry.sequence,
                )
            })
            .max_by(|a, b| a.1.cmp(&b.1).then_with(|| b.2.cmp(&a.2)));

        let result = if let Some((index, priority, _)) = selected {
            let entry = &mut self.entries[index];
            entry.state = JobState::Completed;
            Some(Dispatch {
                id: entry.job.id.clone(),
                effective_priority: priority,
            })
        } else {
            None
        };

        self.last_time = now;
        Ok(result)
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

fn effective_priority(job: &Job, now: u64, aging_ms: u64) -> u8 {
    if aging_ms == 0 {
        return job.priority;
    }

    let gained = (now - job.ready_at) / aging_ms;
    let remaining = 9 - job.priority;
    if gained >= u64::from(remaining) {
        9
    } else {
        job.priority + gained as u8
    }
}