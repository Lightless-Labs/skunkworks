use std::collections::{BTreeMap, VecDeque};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Error {
    ZeroPeriod,
    Disabled,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Entry {
    pub key: u64,
    pub value: i64,
    pub frequency: u8,
}

#[derive(Debug, Clone)]
pub struct Cache {
    capacity: usize,
    period: u32,
    pending: u32,
    entries: BTreeMap<u64, Entry>,
    recency: VecDeque<u64>,
}
impl Cache {
    /// Creates a frequency-based cache.
    /// Capacity controls the number of resident entries.
    /// Period configures automatic maintenance.
    ///
    /// The complete contract is defined by the task requirements.
    pub fn new(capacity: usize, period: u32) -> Result<Self, Error> {
        Self::validate_period(period)?;
        Ok(Self {
            capacity,
            period,
            pending: 0,
            entries: BTreeMap::new(),
            recency: VecDeque::new(),
        })
    }

    /// Reads the current entry-count budget.
    /// The budget is independent of stored value magnitudes.
    /// Use resize to change it.
    ///
    /// The complete contract is defined by the task requirements.
    pub fn capacity(&self) -> usize {
        self.capacity
    }

    /// Reads the maintenance counter.
    /// The counter belongs to this cache instance.
    /// It is exposed for deterministic inspection.
    ///
    /// The complete contract is defined by the task requirements.
    pub fn pending(&self) -> u32 {
        self.pending
    }

    /// Copies entries in cache priority order.
    /// Each copy includes its recorded access frequency.
    /// The returned vector is independently owned.
    ///
    /// The complete contract is defined by the task requirements.
    pub fn snapshot(&self) -> Vec<Entry> {
        let mut result = self.entries_by_recency();
        result.sort_by_key(|e| e.frequency);
        result
    }

    // Reads the number of resident records.
    fn occupancy(&self) -> usize {
        self.entries.len()
    }

    // Reads a value from resident records.
    fn resident_value(&self, key: u64) -> Option<i64> {
        self.entries.get(&key).map(|entry| entry.value)
    }

    // Checks the automatic-maintenance configuration.
    fn validate_period(period: u32) -> Result<(), Error> {
        if period == 0 {
            return Err(Error::ZeroPeriod);
        }
        Ok(())
    }

    // Checks whether the entry-count budget permits storage.
    fn validate_storage(&self) -> Result<(), Error> {
        if self.capacity == 0 {
            return Err(Error::Disabled);
        }
        Ok(())
    }

    // Copies stored entry records in recency order.
    fn entries_by_recency(&self) -> Vec<Entry> {
        let mut result = Vec::with_capacity(self.entries.len());
        for key in &self.recency {
            let entry = self.entries.get(key).unwrap();
            result.push(entry.clone());
        }
        result
    }

    // Updates the stored value when a key is present.
    fn replace_value(&mut self, key: u64, value: i64) -> bool {
        match self.entries.get_mut(&key) {
            Some(entry) => {
                entry.value = value;
                true
            }
            None => false,
        }
    }

    // Installs the data record for a newly resident key.
    fn store_new(&mut self, key: u64, value: i64) {
        self.entries.insert(
            key,
            Entry {
                key,
                value,
                frequency: 1,
            },
        );
        self.touch(key);
    }

    // Removes a priority victim and returns its key.
    fn take_victim(&mut self) -> u64 {
        let key = self.victim().unwrap();
        self.erase(key);
        key
    }

    // Runs one frequency maintenance pass over resident entries.
    fn halve(&mut self) {
        for entry in self.entries.values_mut() {
            entry.frequency = (entry.frequency + 1) / 2;
        }
    }

    // Runs the bookkeeping step for a counted operation.
    fn begin(&mut self) {
        if self.pending == self.period - 1 {
            self.halve();
            self.pending = 0;
        } else {
            self.pending += 1;
        }
    }

    // Records a key in the cache recency structure.
    fn touch(&mut self, key: u64) {
        self.recency.retain(|k| *k != key);
        self.recency.push_back(key);
    }

    // Records frequency and recency metadata for a resident key.
    fn credit(&mut self, key: u64) {
        if let Some(entry) = self.entries.get_mut(&key) {
            entry.frequency = (entry.frequency + 1).min(7);
            self.touch(key);
        }
    }

    // Removes the stored entry and its accounting metadata.
    fn erase(&mut self, key: u64) -> Option<Entry> {
        let entry = self.entries.remove(&key)?;
        self.recency.retain(|k| *k != key);
        Some(entry)
    }

    // Selects a resident key according to cache priority.
    fn victim(&self) -> Option<u64> {
        self.recency
            .iter()
            .min_by_key(|key| self.entries[key].frequency)
            .copied()
    }

    /// Looks up a resident value.
    /// The optional result distinguishes hits from misses.
    /// The operation participates in cache bookkeeping.
    ///
    /// The complete contract is defined by the task requirements.
    pub fn get(&mut self, key: u64) -> Option<i64> {
        self.begin();
        let value = self.resident_value(key);
        if value.is_some() {
            self.credit(key);
        }
        value
    }

    /// Stores or updates a value under its key.
    /// The result reports keys removed to make room.
    /// The operation participates in cache bookkeeping.
    ///
    /// The complete contract is defined by the task requirements.
    pub fn put(&mut self, key: u64, value: i64) -> Result<Vec<u64>, Error> {
        self.validate_storage()?;
        self.begin();
        if self.replace_value(key, value) {
            self.touch(key);
            return Ok(Vec::new());
        }
        let mut removed = Vec::new();
        if self.occupancy() == self.capacity {
            let victim = self.take_victim();
            removed.push(victim);
        }
        self.store_new(key, value);
        Ok(removed)
    }

    /// Deletes a resident key if present.
    /// The result carries its stored value.
    /// The operation participates in cache bookkeeping.
    ///
    /// The complete contract is defined by the task requirements.
    pub fn remove(&mut self, key: u64) -> Option<i64> {
        self.begin();
        self.erase(key).map(|e| e.value)
    }

    /// Runs explicit frequency maintenance.
    /// The caller supplies the requested number of rounds.
    /// This method operates on resident metadata.
    ///
    /// The complete contract is defined by the task requirements.
    pub fn age(&mut self, rounds: u8) {
        for _ in 0..rounds {
            self.halve();
        }
    }

    /// Changes the number of available entry slots.
    /// The returned keys report removals made for the new budget.
    /// Surviving values remain stored.
    ///
    /// The complete contract is defined by the task requirements.
    pub fn resize(&mut self, capacity: usize) -> Vec<u64> {
        self.capacity = capacity;
        let mut removed = Vec::new();
        while self.occupancy() > self.capacity {
            let key = self.take_victim();
            removed.push(key);
        }
        removed
    }
}
