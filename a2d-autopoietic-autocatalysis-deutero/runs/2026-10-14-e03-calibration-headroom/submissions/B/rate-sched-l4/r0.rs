#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Job {
    pub id: u64,
    pub priority: u32,
    pub ready_at: u64,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Dispatch {
    pub job: Job,
    pub enqueued_at: u64,
    pub started_at: u64,
    pub effective_priority: u32,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SchedulerError {
    ZeroAgingInterval,
    TimeWentBackwards,
    DuplicateId(u64),
}

#[derive(Clone, Debug)]
struct PendingJob {
    job: Job,
    enqueued_at: u64,
    insertion_order: u128,
}

pub struct Scheduler {
    aging_interval_ms: u64,
    last_time: u64,
    next_insertion_order: u128,
    pending: Vec<PendingJob>,
}

impl Scheduler {
    pub fn new(aging_interval_ms: u64, start_ms: u64) -> Result<Self, SchedulerError> {
        if aging_interval_ms == 0 {
            return Err(SchedulerError::ZeroAgingInterval);
        }

        Ok(Self {
            aging_interval_ms,
            last_time: start_ms,
            next_insertion_order: 0,
            pending: Vec::new(),
        })
    }

    pub fn enqueue(&mut self, job: Job, now: u64) -> Result<(), SchedulerError> {
        if now < self.last_time {
            return Err(SchedulerError::TimeWentBackwards);
        }
        if self.pending.iter().any(|pending| pending.job.id == job.id) {
            return Err(SchedulerError::DuplicateId(job.id));
        }

        let insertion_order = self.next_insertion_order;
        self.pending.push(PendingJob {
            job,
            enqueued_at: now,
            insertion_order,
        });
        self.next_insertion_order += 1;
        self.last_time = now;
        Ok(())
    }

    pub fn next_ready(&mut self, now: u64) -> Result<Option<Dispatch>, SchedulerError> {
        if now < self.last_time {
            return Err(SchedulerError::TimeWentBackwards);
        }

        let mut best: Option<(usize, u32, u128)> = None;

        for (index, pending) in self.pending.iter().enumerate() {
            if pending.job.ready_at > now {
                continue;
            }

            let age_steps = (now - pending.enqueued_at) / self.aging_interval_ms;
            let effective_priority = (u64::from(pending.job.priority) + age_steps)
                .min(u64::from(u32::MAX)) as u32;

            match best {
                None => best = Some((index, effective_priority, pending.insertion_order)),
                Some((_, best_priority, best_order))
                    if effective_priority > best_priority
                        || (effective_priority == best_priority
                            && pending.insertion_order < best_order) =>
                {
                    best = Some((index, effective_priority, pending.insertion_order));
                }
                _ => {}
            }
        }

        self.last_time = now;

        if let Some((index, effective_priority, _)) = best {
            let pending = self.pending.remove(index);
            Ok(Some(Dispatch {
                job: pending.job,
                enqueued_at: pending.enqueued_at,
                started_at: now,
                effective_priority,
            }))
        } else {
            Ok(None)
        }
    }

    pub fn cancel(&mut self, id: u64, now: u64) -> Result<Option<Job>, SchedulerError> {
        if now < self.last_time {
            return Err(SchedulerError::TimeWentBackwards);
        }

        let result = if let Some(index) = self.pending.iter().position(|pending| pending.job.id == id)
        {
            Some(self.pending.remove(index).job)
        } else {
            None
        };

        self.last_time = now;
        Ok(result)
    }

    pub fn len(&self) -> usize {
        self.pending.len()
    }
}