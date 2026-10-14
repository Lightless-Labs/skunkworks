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

pub struct Scheduler {
    aging_ms: u64,
    last_time: u64,
    next_sequence: u64,
    jobs: Vec<Entry>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum State {
    Queued,
    Cancelled,
    Completed,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct Entry {
    job: Job,
    sequence: u64,
    state: State,
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
            state: State::Queued,
        });
        self.next_sequence += 1;
        self.last_time = now;
        Ok(())
    }

    pub fn cancel(&mut self, now: u64, id: &str) -> Result<(), Error> {
        self.check_time(now)?;

        let entry = match self.jobs.iter_mut().find(|entry| entry.job.id == id) {
            Some(entry) => entry,
            None => return Err(Error::Unknown),
        };

        match entry.state {
            State::Queued => entry.state = State::Cancelled,
            State::Cancelled => return Err(Error::Cancelled),
            State::Completed => return Err(Error::Completed),
        }

        self.last_time = now;
        Ok(())
    }

    pub fn next_ready(&mut self, now: u64) -> Result<Option<Dispatch>, Error> {
        self.check_time(now)?;

        let mut best: Option<(usize, u8, u64)> = None;

        for (index, entry) in self.jobs.iter().enumerate() {
            if entry.state != State::Queued || entry.job.ready_at > now {
                continue;
            }

            let effective_priority = if self.aging_ms == 0 {
                entry.job.priority
            } else {
                let age_steps = (now - entry.job.ready_at) / self.aging_ms;
                (entry.job.priority as u64 + age_steps).min(9) as u8
            };

            match best {
                None => best = Some((index, effective_priority, entry.sequence)),
                Some((_, best_priority, best_sequence))
                    if effective_priority > best_priority
                        || (effective_priority == best_priority
                            && entry.sequence < best_sequence) =>
                {
                    best = Some((index, effective_priority, entry.sequence));
                }
                _ => {}
            }
        }

        self.last_time = now;

        if let Some((index, effective_priority, _)) = best {
            let entry = &mut self.jobs[index];
            entry.state = State::Completed;
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
}

fn valid_id(id: &str) -> bool {
    let bytes = id.as_bytes();
    (1..=16).contains(&bytes.len())
        && bytes
            .iter()
            .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || *byte == b'-')
}