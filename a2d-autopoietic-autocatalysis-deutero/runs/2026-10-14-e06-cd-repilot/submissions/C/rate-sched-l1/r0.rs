use std::collections::HashMap;

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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
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
    entries: HashMap<String, Entry>,
}

impl Scheduler {
    /// Creates an independent scheduler with the given aging interval.
    pub fn new(aging_ms: u64) -> Self {
        Self {
            aging_ms,
            last_time: 0,
            next_sequence: 0,
            entries: HashMap::new(),
        }
    }

    /// Inserts a job, permanently reserving its identifier.
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
                status: Status::Queued,
            },
        );
        self.next_sequence = next_sequence;
        self.last_time = now;
        Ok(())
    }

    /// Cancels a queued job, including one that is not ready yet.
    pub fn cancel(&mut self, now: u64, id: &str) -> Result<(), Error> {
        if now < self.last_time {
            return Err(Error::TimeReversed);
        }

        let status = self.entries.get(id).ok_or(Error::Unknown)?.status;
        match status {
            Status::Queued => {
                self.entries.get_mut(id).expect("entry was checked").status = Status::Cancelled;
                self.last_time = now;
                Ok(())
            }
            Status::Cancelled => Err(Error::Cancelled),
            Status::Completed => Err(Error::Completed),
        }
    }

    /// Dispatches the ready job with the highest effective priority, if any.
    pub fn next_ready(&mut self, now: u64) -> Result<Option<Dispatch>, Error> {
        if now < self.last_time {
            return Err(Error::TimeReversed);
        }

        let selected = self
            .entries
            .iter()
            .filter(|(_, entry)| entry.status == Status::Queued && entry.job.ready_at <= now)
            .map(|(id, entry)| {
                (
                    id.clone(),
                    effective_priority(&entry.job, now, self.aging_ms),
                    entry.sequence,
                )
            })
            .max_by(|a, b| a.1.cmp(&b.1).then_with(|| b.2.cmp(&a.2)));

        self.last_time = now;
        let Some((id, priority, _)) = selected else {
            return Ok(None);
        };

        self.entries.get_mut(&id).expect("selected entry exists").status = Status::Completed;
        Ok(Some(Dispatch {
            id,
            effective_priority: priority,
        }))
    }
}

fn valid_id(id: &str) -> bool {
    (1..=16).contains(&id.len())
        && id
            .bytes()
            .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'-')
}

fn effective_priority(job: &Job, now: u64, aging_ms: u64) -> u8 {
    if aging_ms == 0 {
        return job.priority;
    }
    let aging = (now - job.ready_at) / aging_ms;
    (job.priority as u64 + aging).min(9) as u8
}

fn main() {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dispatches_highest_priority() {
        let mut scheduler = Scheduler::new(0);
        scheduler
            .insert(
                0,
                Job {
                    id: "first".into(),
                    ready_at: 0,
                    priority: 2,
                },
            )
            .unwrap();
        scheduler
            .insert(
                0,
                Job {
                    id: "second".into(),
                    ready_at: 0,
                    priority: 7,
                },
            )
            .unwrap();

        assert_eq!(
            scheduler.next_ready(0).unwrap(),
            Some(Dispatch {
                id: "second".into(),
                effective_priority: 7,
            })
        );
    }

    #[test]
    fn rejects_invalid_id_without_advancing_time() {
        let mut scheduler = Scheduler::new(0);
        assert_eq!(
            scheduler.insert(
                10,
                Job {
                    id: "Bad".into(),
                    ready_at: 0,
                    priority: 1,
                }
            ),
            Err(Error::BadId)
        );
        assert_eq!(scheduler.next_ready(0), Ok(None));
    }

    #[test]
    fn cancel_and_duplicate_reservation_work_end_to_end() {
        let mut scheduler = Scheduler::new(10);
        let job = Job {
            id: "job-1".into(),
            ready_at: 100,
            priority: 4,
        };
        scheduler.insert(0, job.clone()).unwrap();
        scheduler.cancel(1, "job-1").unwrap();
        assert_eq!(scheduler.next_ready(100), Ok(None));
        assert_eq!(scheduler.insert(100, job), Err(Error::Duplicate));
    }

    #[test]
    fn aging_starts_at_readiness_and_saturates() {
        let mut scheduler = Scheduler::new(10);
        scheduler
            .insert(
                0,
                Job {
                    id: "aged".into(),
                    ready_at: 10,
                    priority: 2,
                },
            )
            .unwrap();

        assert_eq!(
            scheduler.next_ready(40).unwrap(),
            Some(Dispatch {
                id: "aged".into(),
                effective_priority: 5,
            })
        );
    }
}