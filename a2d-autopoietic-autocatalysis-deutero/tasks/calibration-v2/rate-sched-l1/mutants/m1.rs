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
struct Entry {
    job: Job,
    state: u8,
}
pub struct Scheduler {
    clock: u64,
    aging: u64,
    entries: Vec<Entry>,
}
impl Scheduler {
    pub fn new(aging_ms: u64) -> Self {
        Self {
            clock: 0,
            aging: aging_ms,
            entries: Vec::new(),
        }
    }
    fn time(&self, now: u64) -> Result<(), Error> {
        if now < self.clock {
            Err(Error::TimeReversed)
        } else {
            Ok(())
        }
    }
    pub fn insert(&mut self, now: u64, job: Job) -> Result<(), Error> {
        self.time(now)?;
        if job.id.is_empty()
            || job.id.len() > 16
            || !job
                .id
                .bytes()
                .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
        {
            return Err(Error::BadId);
        }
        if job.priority > 9 {
            return Err(Error::BadPriority);
        }
        if self.entries.iter().any(|e| e.job.id == job.id) {
            return Err(Error::Duplicate);
        }
        self.entries.push(Entry { job, state: 0 });
        self.clock = now;
        Ok(())
    }
    pub fn cancel(&mut self, now: u64, id: &str) -> Result<(), Error> {
        self.time(now)?;
        let e = self
            .entries
            .iter_mut()
            .find(|e| e.job.id == id)
            .ok_or(Error::Unknown)?;
        match e.state {
            1 => return Err(Error::Cancelled),
            2 => return Err(Error::Completed),
            _ => (),
        }
        e.state = 1;
        self.clock = now;
        Ok(())
    }
    pub fn next_ready(&mut self, now: u64) -> Result<Option<Dispatch>, Error> {
        self.time(now)?;
        let mut best: Option<(usize, u8)> = None;
        for (i, e) in self.entries.iter().enumerate() {
            if e.state != 0 || e.job.ready_at > now {
                continue;
            }
            let age = if self.aging == 0 {
                0
            } else {
                (now - e.job.ready_at) / self.aging
            };
            let p = (e.job.priority as u64).saturating_add(age).min(9) as u8;
            if best.is_none_or(|(_, p0)| p >= p0) {
                best = Some((i, p));
            }
        }
        self.clock = now;
        Ok(best.map(|(i, p)| {
            self.entries[i].state = 2;
            Dispatch {
                id: self.entries[i].job.id.clone(),
                effective_priority: p,
            }
        }))
    }
}
