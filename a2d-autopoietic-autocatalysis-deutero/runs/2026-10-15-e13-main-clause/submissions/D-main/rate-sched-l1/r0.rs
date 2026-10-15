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
    sequence: u64,
    state: JobState,
}

pub struct Scheduler {
    aging_ms: u64,
    last_time: u64,
    next_sequence: u64,
    jobs: Vec<Entry>,
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
        self.check_time(now)?;

        if !valid_id(&job.id) {
            return Err(Error::BadId);
        }
        if job.priority > 9 {
            return Err(Error::BadPriority);
        }
        if self.jobs.iter().any(|entry| entry.job.id == job.id) {
            return Err(Error::Duplicate);
        }

        self.jobs.push(Entry {
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

        let Some(entry) = self.jobs.iter_mut().find(|entry| entry.job.id == id) else {
            return Err(Error::Unknown);
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

        let selected = self
            .jobs
            .iter()
            .enumerate()
            .filter(|(_, entry)| {
                entry.state == JobState::Queued && entry.job.ready_at <= now
            })
            .map(|(index, entry)| {
                (index, effective_priority(&entry.job, now, self.aging_ms), entry.sequence)
            })
            .max_by(|a, b| a.1.cmp(&b.1).then_with(|| b.2.cmp(&a.2)));

        let result = if let Some((index, priority, _)) = selected {
            self.jobs[index].state = JobState::Completed;
            Some(Dispatch {
                id: self.jobs[index].job.id.clone(),
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
    !bytes.is_empty()
        && bytes.len() <= 16
        && bytes
            .iter()
            .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || *byte == b'-')
}

fn effective_priority(job: &Job, now: u64, aging_ms: u64) -> u8 {
    if aging_ms == 0 {
        return job.priority;
    }

    let aging = (now - job.ready_at) / aging_ms;
    (job.priority as u64 + aging).min(9) as u8
}

fn main() {}