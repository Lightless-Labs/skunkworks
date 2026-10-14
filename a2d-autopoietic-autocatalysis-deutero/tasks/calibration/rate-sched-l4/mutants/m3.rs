#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Job { pub id: u64, pub priority: u32, pub ready_at: u64 }
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Dispatch { pub job: Job, pub enqueued_at: u64, pub started_at: u64, pub effective_priority: u32 }
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SchedulerError { ZeroAgingInterval, TimeWentBackwards, DuplicateId(u64) }
struct Pending { job: Job, enqueued_at: u64 }
pub struct Scheduler { aging_interval: u64, last: u64, jobs: Vec<Pending> }
impl Scheduler {
    pub fn new(aging_interval_ms: u64, start_ms: u64) -> Result<Self, SchedulerError> {
        if aging_interval_ms == 0 { return Err(SchedulerError::ZeroAgingInterval); }
        Ok(Self { aging_interval: aging_interval_ms, last: start_ms, jobs: Vec::new() })
    }
    fn check_time(&self, now: u64) -> Result<(), SchedulerError> {
        if now < self.last { Err(SchedulerError::TimeWentBackwards) } else { Ok(()) }
    }
    pub fn enqueue(&mut self, job: Job, now: u64) -> Result<(), SchedulerError> {
        self.check_time(now)?;
        if self.jobs.iter().any(|entry| entry.job.id == job.id) {
            return Err(SchedulerError::DuplicateId(job.id));
        }
        self.last = now;
        self.jobs.push(Pending { job, enqueued_at: now });
        Ok(())
    }
    pub fn next_ready(&mut self, now: u64) -> Result<Option<Dispatch>, SchedulerError> {
        self.check_time(now)?;
        self.last = now;
        let mut best: Option<(usize, u32)> = None;
        for (index, entry) in self.jobs.iter().enumerate() {
            if entry.job.ready_at > now { continue; }
            let steps = (now - entry.enqueued_at) / self.aging_interval;
            let effective = entry.job.priority.saturating_add(steps.min(u32::MAX as u64) as u32);
            if best.map_or(true, |(i, p)| effective > p || (effective == p && entry.job.id < self.jobs[i].job.id)) {
                best = Some((index, effective));
            }
        }
        if let Some((index, effective_priority)) = best {
            let entry = self.jobs.remove(index);
            Ok(Some(Dispatch { job: entry.job, enqueued_at: entry.enqueued_at, started_at: now, effective_priority }))
        } else { Ok(None) }
    }
    pub fn cancel(&mut self, id: u64, now: u64) -> Result<Option<Job>, SchedulerError> {
        self.check_time(now)?;
        self.last = now;
        Ok(self.jobs.iter().position(|entry| entry.job.id == id).map(|index| self.jobs.remove(index).job))
    }
    pub fn len(&self) -> usize { self.jobs.len() }
}
