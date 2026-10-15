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

        let id_valid = (1..=16).contains(&job.id.len())
            && job
                .id
                .bytes()
                .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-');
        if !id_valid {
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
            state: JobState::Queued,
        });
        self.last_time = now;
        Ok(())
    }

    pub fn cancel(&mut self, now: u64, id: &str) -> Result<(), Error> {
        if now < self.last_time {
            return Err(Error::TimeReversed);
        }

        let Some(index) = self.entries.iter().position(|entry| entry.job.id == id) else {
            return Err(Error::Unknown);
        };

        match self.entries[index].state {
            JobState::Queued => self.entries[index].state = JobState::Cancelled,
            JobState::Cancelled => return Err(Error::Cancelled),
            JobState::Completed => return Err(Error::Completed),
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
            if entry.state != JobState::Queued || entry.job.ready_at > now {
                continue;
            }

            let effective = if self.aging_ms == 0 {
                entry.job.priority
            } else {
                let age = (now - entry.job.ready_at) / self.aging_ms;
                (entry.job.priority as u128 + age as u128).min(9) as u8
            };

            let should_select = match selected {
                None => true,
                Some((_, best_priority, best_sequence)) => {
                    effective > best_priority
                        || (effective == best_priority && entry.sequence < best_sequence)
                }
            };
            if should_select {
                selected = Some((index, effective, entry.sequence));
            }
        }

        self.last_time = now;
        if let Some((index, effective_priority, _)) = selected {
            self.entries[index].state = JobState::Completed;
            Ok(Some(Dispatch {
                id: self.entries[index].job.id.clone(),
                effective_priority,
            }))
        } else {
            Ok(None)
        }
    }
}

fn main() {}