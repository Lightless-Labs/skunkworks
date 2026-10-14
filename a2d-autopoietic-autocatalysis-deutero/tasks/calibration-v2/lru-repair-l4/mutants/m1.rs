use std::collections::{BTreeMap, VecDeque};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Error {
    ZeroWeight,
    TooHeavy { weight: u64, capacity: u64 },
    Missing { key: u64 },
    Dirty { key: u64 },
    FlushFailed { key: u64 },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Event {
    Hit { key: u64 },
    Miss { key: u64 },
    Store { key: u64, dirty: bool },
    Dirty { key: u64 },
    FlushStart { key: u64, value: i64 },
    FlushOk { key: u64 },
    FlushErr { key: u64 },
    Evict { key: u64 },
    Remove { key: u64 },
    Resize { capacity: u64 },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Entry {
    pub key: u64,
    pub value: i64,
    pub weight: u64,
    pub dirty: bool,
}

#[derive(Debug, Clone)]
pub struct Cache {
    capacity: u64,
    used: u64,
    entries: BTreeMap<u64, Entry>,
    recency: VecDeque<u64>,
    dirty_order: VecDeque<u64>,
    events: Vec<Event>,
}
impl Cache {
    /// Creates a weighted write-back cache.
    /// The caller supplies the initial storage budget.
    /// The cache owns entries, metadata and event history.
    ///
    /// The complete contract is defined by the task requirements.
    pub fn new(capacity: u64) -> Self {
        Self {
            capacity,
            used: 0,
            entries: BTreeMap::new(),
            recency: VecDeque::new(),
            dirty_order: VecDeque::new(),
            events: Vec::new(),
        }
    }

    /// Reads the stored capacity setting.
    /// The result is expressed in entry-weight units.
    /// Use resize to change this setting.
    ///
    /// The complete contract is defined by the task requirements.
    pub fn capacity(&self) -> u64 {
        self.capacity
    }

    /// Reads total resident weight.
    /// Clean and dirty entries both contribute to this total.
    /// Individual weights can be inspected through snapshot.
    ///
    /// The complete contract is defined by the task requirements.
    pub fn used(&self) -> u64 {
        self.used
    }

    /// Copies resident entries in cache order.
    /// Each copy includes its cleanliness metadata.
    /// The returned vector is independently owned.
    ///
    /// The complete contract is defined by the task requirements.
    pub fn snapshot(&self) -> Vec<Entry> {
        self.recency
            .iter()
            .map(|k| self.entries[k].clone())
            .collect()
    }

    /// Copies the accumulated operation events.
    /// The returned event vector is independently owned.
    /// Use drain_events to consume the stored history.
    ///
    /// The complete contract is defined by the task requirements.
    pub fn events(&self) -> Vec<Event> {
        self.events.clone()
    }

    /// Consumes the accumulated event history.
    /// The returned vector contains the removed events.
    /// Subsequent operations can append a new history.
    ///
    /// The complete contract is defined by the task requirements.
    pub fn drain_events(&mut self) -> Vec<Event> {
        std::mem::take(&mut self.events)
    }

    // Records a key in the cache recency structure.
    fn touch(&mut self, key: u64) {
        self.recency.retain(|k| *k != key);
        self.recency.push_back(key);
    }

    // Records a key in the write-back scheduling structure.
    fn dirty_touch(&mut self, key: u64) {
        self.dirty_order.retain(|k| *k != key);
        self.dirty_order.push_back(key);
    }

    // Removes the stored entry and its accounting metadata.
    fn erase(&mut self, key: u64) -> Option<Entry> {
        let e = self.entries.remove(&key)?;
        self.used -= e.weight;
        self.recency.retain(|k| *k != key);
        self.dirty_order.retain(|k| *k != key);
        Some(e)
    }

    // Performs write-back bookkeeping for one resident key.
    fn flush_key<F>(&mut self, key: u64, flush: &mut F) -> Result<(), Error>
    where
        F: FnMut(u64, i64) -> bool,
    {
        let entry = &self.entries[&key];
        if !entry.dirty {
            return Ok(());
        }
        let value = entry.value;
        self.events.push(Event::FlushStart { key, value });
        if !flush(key, value) {
            self.events.push(Event::FlushErr { key });
            return Err(Error::FlushFailed { key });
        }
        self.entries.get_mut(&key).unwrap().dirty = false;
        self.dirty_order.retain(|k| *k != key);
        self.events.push(Event::FlushOk { key });
        Ok(())
    }

    // Performs the steps needed to remove one cache victim.
    fn evict_key<F>(&mut self, key: u64, flush: &mut F) -> Result<(), Error>
    where
        F: FnMut(u64, i64) -> bool,
    {
        self.flush_key(key, flush)?;
        self.erase(key);
        self.events.push(Event::Evict { key });
        Ok(())
    }

    /// Looks up the value associated with a key.
    /// The returned option distinguishes a hit from a miss.
    /// The lookup is included in the event history.
    ///
    /// The complete contract is defined by the task requirements.
    pub fn get(&mut self, key: u64) -> Option<i64> {
        let value = self.entries.get(&key).map(|e| e.value);
        if value.is_some() {
            self.touch(key);
            self.events.push(Event::Hit { key });
        } else {
            self.events.push(Event::Miss { key });
        }
        value
    }

    /// Records a write requirement for an existing key.
    /// This operation changes entry metadata.
    /// The result reports whether the key exists.
    ///
    /// The complete contract is defined by the task requirements.
    pub fn mark_dirty(&mut self, key: u64) -> Result<(), Error> {
        let entry = self.entries.get_mut(&key).ok_or(Error::Missing { key })?;
        entry.dirty = true;
        self.dirty_touch(key);
        self.events.push(Event::Dirty { key });
        Ok(())
    }

    /// Requests explicit deletion of a clean key.
    /// Successful deletion returns the stored value.
    /// The result reports keys that cannot be removed this way.
    ///
    /// The complete contract is defined by the task requirements.
    pub fn remove(&mut self, key: u64) -> Result<i64, Error> {
        let entry = self.entries.get(&key).ok_or(Error::Missing { key })?;
        if entry.dirty {
            return Err(Error::Dirty { key });
        }
        let value = self.erase(key).unwrap().value;
        self.events.push(Event::Remove { key });
        Ok(value)
    }

    /// Stores a value with weight and cleanliness metadata.
    /// The callback performs any write-back required by storage.
    /// The result reports keys evicted during the operation.
    ///
    /// The complete contract is defined by the task requirements.
    pub fn put<F>(
        &mut self,
        key: u64,
        value: i64,
        weight: u64,
        dirty: bool,
        flush: &mut F,
    ) -> Result<Vec<u64>, Error>
    where
        F: FnMut(u64, i64) -> bool,
    {
        if weight == 0 {
            return Err(Error::ZeroWeight);
        }
        if weight > self.capacity {
            return Err(Error::TooHeavy {
                weight,
                capacity: self.capacity,
            });
        }
        if !dirty && self.entries.get(&key).is_some_and(|e| e.dirty) {
            self.flush_key(key, flush)?;
        }
        let old_weight = self.entries.get(&key).map_or(0, |e| e.weight);
        let mut removed = Vec::new();
        while self.used - old_weight > self.capacity - weight {
            let victim = *self.recency.iter().find(|k| **k != key).unwrap();
            self.evict_key(victim, flush)?;
            removed.push(victim);
        }
        self.erase(key);
        self.used += weight;
        self.entries.insert(
            key,
            Entry {
                key,
                value,
                weight,
                dirty,
            },
        );
        self.touch(key);
        if dirty {
            self.dirty_touch(key);
        }
        self.events.push(Event::Store { key, dirty });
        Ok(removed)
    }

    /// Requests write-back of resident dirty data.
    /// The limit bounds the amount of requested work.
    /// The callback performs the individual writes.
    ///
    /// The complete contract is defined by the task requirements.
    pub fn flush<F>(&mut self, limit: usize, flush: &mut F) -> Result<Vec<u64>, Error>
    where
        F: FnMut(u64, i64) -> bool,
    {
        let mut completed = Vec::new();
        while completed.len() < limit {
            let Some(key) = self.recency.iter().find(|k| self.entries[k].dirty).copied() else {
                break;
            };
            self.flush_key(key, flush)?;
            completed.push(key);
        }
        Ok(completed)
    }

    /// Requests removal down to a resident-weight target.
    /// The callback performs any required write-back.
    /// The result reports completed removals on success.
    ///
    /// The complete contract is defined by the task requirements.
    pub fn evict_to<F>(&mut self, target: u64, flush: &mut F) -> Result<Vec<u64>, Error>
    where
        F: FnMut(u64, i64) -> bool,
    {
        let mut removed = Vec::new();
        while self.used > target {
            let key = *self.recency.front().unwrap();
            self.evict_key(key, flush)?;
            removed.push(key);
        }
        Ok(removed)
    }

    /// Requests a new storage budget.
    /// The callback performs any required write-back.
    /// The result reports completed removals on success.
    ///
    /// The complete contract is defined by the task requirements.
    pub fn resize<F>(&mut self, capacity: u64, flush: &mut F) -> Result<Vec<u64>, Error>
    where
        F: FnMut(u64, i64) -> bool,
    {
        let removed = self.evict_to(capacity, flush)?;
        self.capacity = capacity;
        self.events.push(Event::Resize { capacity });
        Ok(removed)
    }
}
