use std::collections::HashMap;

/// A job waiting to be dispatched.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Job {
    pub id: String,
    pub ready_at: u64,
    pub priority: u8,
}

/// A job selected for dispatch.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Dispatch {
    pub id: String,
    pub effective_priority: u8,
}

/// Errors returned by scheduler operations.
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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum State {
    Queued,
    Cancelled,
    Completed,
}

struct Entry {
    job: Job,
    sequence: u64,
    state: State,
}

/// Schedules ready jobs by effective priority, then insertion order.
pub struct Scheduler {
    aging_ms: u64,
    last_time: u64,
    next_sequence: u64,
    entries: HashMap<String, Entry>,
}

impl Scheduler {
    /// Creates an empty scheduler with the given aging interval in milliseconds.
    pub fn new(aging_ms: u64) -> Self {
        Self {
            aging_ms,
            last_time: 0,
            next_sequence: 0,
            entries: HashMap::new(),
        }
    }

    /// Inserts a job, reserving its ID permanently after successful insertion.
    pub fn insert(&mut self, now: u64, job: Job) -> Result<(), Error> {
        if now < self.last_time {
            return Err(Error::TimeReversed);
        }
        if !valid_id(&job.id) {
            return Err(Error::BadId);
        }
        if job.priority > 9 {
            return Err(Error::BadPriority);
        }
        if self.entries.contains_key(&job.id) {
            return Err(Error::Duplicate);
        }

        let sequence = self.next_sequence;
        let next_sequence = sequence.checked_add(1).ok_or(Error::Duplicate)?;

        self.entries.insert(
            job.id.clone(),
            Entry {
                job,
                sequence,
                state: State::Queued,
            },
        );
        self.next_sequence = next_sequence;
        self.last_time = now;
        Ok(())
    }

    /// Cancels a queued job by its exact ID.
    pub fn cancel(&mut self, now: u64, id: &str) -> Result<(), Error> {
        if now < self.last_time {
            return Err(Error::TimeReversed);
        }

        let entry = self.entries.get(id).ok_or(Error::Unknown)?;
        match entry.state {
            State::Queued => {}
            State::Cancelled => return Err(Error::Cancelled),
            State::Completed => return Err(Error::Completed),
        }

        self.entries.get_mut(id).expect("entry was checked").state = State::Cancelled;
        self.last_time = now;
        Ok(())
    }

    /// Dispatches the highest-priority ready job, if one is available.
    pub fn next_ready(&mut self, now: u64) -> Result<Option<Dispatch>, Error> {
        if now < self.last_time {
            return Err(Error::TimeReversed);
        }

        let selected = self
            .entries
            .iter()
            .filter(|(_, entry)| entry.state == State::Queued && entry.job.ready_at <= now)
            .map(|(id, entry)| {
                (
                    id.clone(),
                    effective_priority(&entry.job, now, self.aging_ms),
                    entry.sequence,
                )
            })
            .max_by(|a, b| a.1.cmp(&b.1).then_with(|| b.2.cmp(&a.2)));

        self.last_time = now;
        let Some((id, effective_priority, _)) = selected else {
            return Ok(None);
        };

        self.entries.get_mut(&id).expect("selected entry exists").state = State::Completed;
        Ok(Some(Dispatch {
            id,
            effective_priority,
        }))
    }
}

fn valid_id(id: &str) -> bool {
    let bytes = id.as_bytes();
    (1..=16).contains(&bytes.len())
        && bytes
            .iter()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || *b == b'-')
}

fn effective_priority(job: &Job, now: u64, aging_ms: u64) -> u8 {
    if aging_ms == 0 {
        return job.priority;
    }
    let age = (now - job.ready_at) / aging_ms;
    let remaining = 9 - job.priority as u64;
    if age >= remaining {
        9
    } else {
        job.priority + age as u8
    }
}

fn main() {}

#[cfg(test)]
mod tests {
    use super::*;

    fn job(id: &str, ready_at: u64, priority: u8) -> Job {
        Job {
            id: id.to_string(),
            ready_at,
            priority,
        }
    }

    #[test]
    fn dispatches_ready_job_and_reports_priority() {
        let mut scheduler = Scheduler::new(0);
        scheduler.insert(0, job("alpha", 0, 4)).unwrap();
        assert_eq!(
            scheduler.next_ready(0).unwrap(),
            Some(Dispatch {
                id: "alpha".to_string(),
                effective_priority: 4
            })
        );
    }

    #[test]
    fn rejects_invalid_id_and_preserves_clock() {
        let mut scheduler = Scheduler::new(10);
        assert_eq!(scheduler.insert(5, job("Bad", 0, 1)), Err(Error::BadId));
        assert_eq!(scheduler.insert(4, job("valid", 0, 1)), Ok(()));
    }

    #[test]
    fn future_job_is_not_ready_until_its_ready_time() {
        let mut scheduler = Scheduler::new(10);
        scheduler.insert(0, job("future", 20, 2)).unwrap();
        assert_eq!(scheduler.next_ready(19).unwrap(), None);
        assert_eq!(
            scheduler.next_ready(20).unwrap(),
            Some(Dispatch {
                id: "future".to_string(),
                effective_priority: 2
            })
        );
    }

    #[test]
    fn aging_saturates_without_overflow() {
        let mut scheduler = Scheduler::new(1);
        scheduler.insert(0, job("old", 0, 8)).unwrap();
        assert_eq!(
            scheduler.next_ready(u64::MAX).unwrap(),
            Some(Dispatch {
                id: "old".to_string(),
                effective_priority: 9
            })
        );
    }
}