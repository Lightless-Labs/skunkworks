use std::collections::HashMap;

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

struct PendingJob {
    job: Job,
    enqueued_at: u64,
    insertion_order: u128,
}

pub struct Scheduler {
    aging_interval_ms: u64,
    last_time: u64,
    next_insertion_order: u128,
    jobs: Vec<PendingJob>,
    id_to_index: HashMap<u64, usize>,
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
            jobs: Vec::new(),
            id_to_index: HashMap::new(),
        })
    }

    fn check_time(&self, now: u64) -> Result<(), SchedulerError> {
        if now < self.last_time {
            Err(SchedulerError::TimeWentBackwards)
        } else {
            Ok(())
        }
    }

    pub fn enqueue(&mut self, job: Job, now: u64) -> Result<(), SchedulerError> {
        self.check_time(now)?;

        if self.id_to_index.contains_key(&job.id) {
            return Err(SchedulerError::DuplicateId(job.id));
        }

        let index = self.jobs.len();
        self.jobs.push(PendingJob {
            job: job.clone(),
            enqueued_at: now,
            insertion_order: self.next_insertion_order,
        });
        self.id_to_index.insert(job.id, index);

        // At most usize::MAX entries can be stored in the Vec, so this
        // increment cannot overflow u128 on supported Rust targets.
        self.next_insertion_order += 1;
        self.last_time = now;
        Ok(())
    }

    pub fn next_ready(&mut self, now: u64) -> Result<Option<Dispatch>, SchedulerError> {
        self.check_time(now)?;

        let mut selected: Option<(usize, u32, u128)> = None;

        for (index, pending) in self.jobs.iter().enumerate() {
            if pending.job.ready_at > now {
                continue;
            }

            let age_steps = (now - pending.enqueued_at) / self.aging_interval_ms;
            let effective_priority = (pending.job.priority as u64)
                .saturating_add(age_steps)
                .min(u32::MAX as u64) as u32;

            let should_select = match selected {
                None => true,
                Some((_, selected_priority, selected_order)) => {
                    effective_priority > selected_priority
                        || (effective_priority == selected_priority
                            && pending.insertion_order < selected_order)
                }
            };

            if should_select {
                selected = Some((index, effective_priority, pending.insertion_order));
            }
        }

        self.last_time = now;

        let Some((index, effective_priority, _)) = selected else {
            return Ok(None);
        };

        let pending = self.jobs.remove(index);
        self.id_to_index.remove(&pending.job.id);

        for (new_index, remaining) in self.jobs.iter().enumerate().skip(index) {
            self.id_to_index.insert(remaining.job.id, new_index);
        }

        Ok(Some(Dispatch {
            job: pending.job,
            enqueued_at: pending.enqueued_at,
            started_at: now,
            effective_priority,
        }))
    }

    pub fn cancel(&mut self, id: u64, now: u64) -> Result<Option<Job>, SchedulerError> {
        self.check_time(now)?;

        let result = if let Some(index) = self.id_to_index.remove(&id) {
            let pending = self.jobs.remove(index);
            for (new_index, remaining) in self.jobs.iter().enumerate().skip(index) {
                self.id_to_index.insert(remaining.job.id, new_index);
            }
            Some(pending.job)
        } else {
            None
        };

        self.last_time = now;
        Ok(result)
    }

    pub fn len(&self) -> usize {
        self.jobs.len()
    }
}