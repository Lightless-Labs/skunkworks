use std::collections::{BTreeMap, VecDeque};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Error {
    BackwardTime { last: u64, now: u64 },
    ZeroWeight,
    TooHeavy { weight: u64, capacity: u64 },
    DeadlineOverflow,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Entry {
    pub key: u64,
    pub value: i64,
    pub weight: u64,
    pub expires_at: Option<u64>,
}

#[derive(Debug, Clone)]
pub struct Cache {
    capacity: u64,
    used: u64,
    clock: u64,
    entries: BTreeMap<u64, Entry>,
    order: VecDeque<u64>,
}
impl Cache {
    /// Creates a weighted cache.
    /// The caller supplies its initial capacity.
    /// The returned cache owns all of its storage.
    ///
    /// The complete contract is defined by the task requirements.
    pub fn new(capacity: u64) -> Self {
        Self {
            capacity,
            used: 0,
            clock: 0,
            entries: BTreeMap::new(),
            order: VecDeque::new(),
        }
    }

    /// Reads the current capacity setting.
    /// This is the weight budget used for storage.
    /// Use resize to change the budget.
    ///
    /// The complete contract is defined by the task requirements.
    pub fn capacity(&self) -> u64 {
        self.capacity
    }

    /// Reads the amount of resident weight.
    /// Weights belong to individual stored entries.
    /// The result is independent of the number of keys.
    ///
    /// The complete contract is defined by the task requirements.
    pub fn used(&self) -> u64 {
        self.used
    }

    /// Reads the stored logical time.
    /// Time is supplied by the caller of each timed operation.
    /// No system clock is consulted.
    ///
    /// The complete contract is defined by the task requirements.
    pub fn clock(&self) -> u64 {
        self.clock
    }

    /// Copies the resident entries into an ordered vector.
    /// Each entry contains its value and storage metadata.
    /// The returned vector is independently owned.
    ///
    /// The complete contract is defined by the task requirements.
    pub fn snapshot(&self) -> Vec<Entry> {
        self.order
            .iter()
            .map(|key| self.entries[key].clone())
            .collect()
    }

    // Reads a value from resident storage.
    fn entry_value(&self, key: u64) -> Option<i64> {
        self.entries.get(&key).map(|entry| entry.value)
    }

    // Checks the requested storage weight.
    fn validate_weight(&self, weight: u64) -> Result<(), Error> {
        if weight == 0 {
            return Err(Error::ZeroWeight);
        }
        if weight > self.capacity {
            return Err(Error::TooHeavy {
                weight,
                capacity: self.capacity,
            });
        }
        Ok(())
    }

    // Builds lifetime metadata from the supplied time arguments.
    fn make_deadline(ttl: Option<u64>, now: u64) -> Result<Option<u64>, Error> {
        match ttl {
            None => Ok(None),
            Some(duration) => {
                let deadline = now.checked_add(duration).ok_or(Error::DeadlineOverflow)?;
                Ok(Some(deadline))
            }
        }
    }

    // Reads the first key in cache recency order.
    fn oldest(&self) -> Option<u64> {
        self.order.front().copied()
    }

    // Collects keys selected by the lifetime metadata.
    fn expired_keys(&self, now: u64) -> Vec<u64> {
        self.entries
            .iter()
            .filter(|(_, entry)| entry.expires_at.is_some_and(|deadline| deadline <= now))
            .map(|(key, _)| *key)
            .collect()
    }

    // Checks the logical-time argument against stored state.
    fn validate_time(&self, now: u64) -> Result<(), Error> {
        if now < self.clock {
            return Err(Error::BackwardTime {
                last: self.clock,
                now,
            });
        }
        Ok(())
    }

    // Removes the stored entry and its accounting metadata.
    fn erase(&mut self, key: u64) -> Option<Entry> {
        let entry = self.entries.remove(&key)?;
        self.used -= entry.weight;
        self.order.retain(|k| *k != key);
        Some(entry)
    }

    // Records a key in the cache recency structure.
    fn touch(&mut self, key: u64) {
        self.order.retain(|k| *k != key);
        self.order.push_back(key);
    }

    // Collects and removes expired resident entries.
    fn expire(&mut self, now: u64) -> Vec<u64> {
        let expired = self.expired_keys(now);
        for key in &expired {
            self.erase(*key);
        }
        self.clock = now;
        expired
    }

    /// Performs maintenance at a caller-supplied time.
    /// The returned keys identify entries removed during maintenance.
    /// No new entry is stored by this operation.
    ///
    /// The complete contract is defined by the task requirements.
    pub fn advance(&mut self, now: u64) -> Result<Vec<u64>, Error> {
        self.validate_time(now)?;
        Ok(self.expire(now))
    }

    /// Looks up a key at a caller-supplied time.
    /// The optional value distinguishes a hit from a miss.
    /// A time-validation error is returned through Result.
    ///
    /// The complete contract is defined by the task requirements.
    pub fn get(&mut self, key: u64, now: u64) -> Result<Option<i64>, Error> {
        self.validate_time(now)?;
        self.expire(now);
        let value = self.entry_value(key);
        if value.is_some() {
            self.touch(key);
        }
        Ok(value)
    }

    /// Deletes a key at a caller-supplied time.
    /// The optional result contains the removed value.
    /// This operation does not create new entries.
    ///
    /// The complete contract is defined by the task requirements.
    pub fn remove(&mut self, key: u64, now: u64) -> Result<Option<i64>, Error> {
        self.validate_time(now)?;
        self.expire(now);
        Ok(self.erase(key).map(|e| e.value))
    }

    /// Stores a value and its storage metadata.
    /// The caller supplies the key, weight, lifetime and time.
    /// The result reports keys removed as part of the operation.
    ///
    /// The complete contract is defined by the task requirements.
    pub fn insert(
        &mut self,
        key: u64,
        value: i64,
        weight: u64,
        ttl: Option<u64>,
        now: u64,
    ) -> Result<Vec<u64>, Error> {
        self.validate_time(now)?;
        self.validate_weight(weight)?;
        let expires_at = Self::make_deadline(ttl, now)?;
        let mut removed = self.expire(now);
        self.erase(key);
        if ttl == Some(0) {
            return Ok(removed);
        }
        while self.used >= self.capacity - weight && self.used > 0 {
            let victim = self.oldest().unwrap();
            self.erase(victim);
            removed.push(victim);
        }
        self.used += weight;
        self.entries.insert(
            key,
            Entry {
                key,
                value,
                weight,
                expires_at,
            },
        );
        self.touch(key);
        Ok(removed)
    }

    /// Changes the weight budget at a caller-supplied time.
    /// The result reports entries removed during this operation.
    /// The new budget may be larger or smaller.
    ///
    /// The complete contract is defined by the task requirements.
    pub fn resize(&mut self, capacity: u64, now: u64) -> Result<Vec<u64>, Error> {
        self.validate_time(now)?;
        let mut removed = self.expire(now);
        self.capacity = capacity;
        while self.used > self.capacity {
            let key = self.oldest().unwrap();
            self.erase(key);
            removed.push(key);
        }
        Ok(removed)
    }
}
