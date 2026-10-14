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
    /// Creates an empty weighted cache with the given capacity.
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

    /// Returns the configured capacity.
    pub fn capacity(&self) -> u64 {
        self.capacity
    }

    /// Returns the total weight of resident entries.
    pub fn used(&self) -> u64 {
        self.used
    }

    /// Returns cloned entries in oldest-to-newest LRU order.
    pub fn snapshot(&self) -> Vec<Entry> {
        self.recency
            .iter()
            .filter_map(|key| self.entries.get(key).cloned())
            .collect()
    }

    /// Returns a cloned copy of the event log.
    pub fn events(&self) -> Vec<Event> {
        self.events.clone()
    }

    /// Returns the event log and clears it.
    pub fn drain_events(&mut self) -> Vec<Event> {
        std::mem::take(&mut self.events)
    }

    fn touch(&mut self, key: u64) {
        self.recency.retain(|&item| item != key);
        self.recency.push_back(key);
    }

    fn dirty_touch(&mut self, key: u64) {
        self.dirty_order.retain(|&item| item != key);
        self.dirty_order.push_back(key);
    }

    fn erase(&mut self, key: u64) -> Option<Entry> {
        let entry = self.entries.remove(&key)?;
        self.used -= entry.weight;
        self.recency.retain(|&item| item != key);
        self.dirty_order.retain(|&item| item != key);
        Some(entry)
    }

    fn flush_key<F>(&mut self, key: u64, flush: &mut F) -> Result<(), Error>
    where
        F: FnMut(u64, i64) -> bool,
    {
        let Some(entry) = self.entries.get(&key) else {
            return Ok(());
        };
        if !entry.dirty {
            return Ok(());
        }

        let value = entry.value;
        self.events.push(Event::FlushStart { key, value });
        if !flush(key, value) {
            self.events.push(Event::FlushErr { key });
            return Err(Error::FlushFailed { key });
        }

        if let Some(entry) = self.entries.get_mut(&key) {
            entry.dirty = false;
        }
        self.dirty_order.retain(|&item| item != key);
        self.events.push(Event::FlushOk { key });
        Ok(())
    }

    fn evict_key<F>(&mut self, key: u64, flush: &mut F) -> Result<(), Error>
    where
        F: FnMut(u64, i64) -> bool,
    {
        self.flush_key(key, flush)?;
        self.erase(key);
        self.events.push(Event::Evict { key });
        Ok(())
    }

    /// Looks up a key, records a hit or miss, and updates LRU order on a hit.
    pub fn get(&mut self, key: u64) -> Option<i64> {
        let value = self.entries.get(&key).map(|entry| entry.value);
        if value.is_some() {
            self.touch(key);
            self.events.push(Event::Hit { key });
        } else {
            self.events.push(Event::Miss { key });
        }
        value
    }

    /// Marks a resident entry dirty and moves it to the newest dirty-write position.
    pub fn mark_dirty(&mut self, key: u64) -> Result<(), Error> {
        let Some(entry) = self.entries.get_mut(&key) else {
            return Err(Error::Missing { key });
        };
        entry.dirty = true;
        self.dirty_touch(key);
        self.events.push(Event::Dirty { key });
        Ok(())
    }

    /// Removes a clean resident entry and returns its value.
    pub fn remove(&mut self, key: u64) -> Result<i64, Error> {
        let Some(entry) = self.entries.get(&key) else {
            return Err(Error::Missing { key });
        };
        if entry.dirty {
            return Err(Error::Dirty { key });
        }
        let value = self.erase(key).expect("entry was checked above").value;
        self.events.push(Event::Remove { key });
        Ok(value)
    }

    /// Stores a value, evicting least-recently-used entries as needed.
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

        let old_weight = self.entries.get(&key).map_or(0, |entry| entry.weight);

        if !dirty && self.entries.get(&key).is_some_and(|entry| entry.dirty) {
            self.flush_key(key, flush)?;
        }

        let target_existing_weight = self.capacity - weight;
        let mut removed = Vec::new();

        while self.used - old_weight > target_existing_weight {
            let Some(victim) = self.recency.iter().copied().find(|&item| item != key) else {
                break;
            };
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

    /// Flushes up to `limit` dirty entries in dirty-write order.
    pub fn flush<F>(&mut self, limit: usize, flush: &mut F) -> Result<Vec<u64>, Error>
    where
        F: FnMut(u64, i64) -> bool,
    {
        let mut completed = Vec::new();
        while completed.len() < limit {
            let Some(key) = self.dirty_order.front().copied() else {
                break;
            };
            self.flush_key(key, flush)?;
            completed.push(key);
        }
        Ok(completed)
    }

    /// Evicts least-recently-used entries until used weight is at most `target`.
    pub fn evict_to<F>(&mut self, target: u64, flush: &mut F) -> Result<Vec<u64>, Error>
    where
        F: FnMut(u64, i64) -> bool,
    {
        let mut removed = Vec::new();
        while self.used > target {
            let Some(key) = self.recency.front().copied() else {
                break;
            };
            self.evict_key(key, flush)?;
            removed.push(key);
        }
        Ok(removed)
    }

    /// Resizes the cache, committing the new capacity only after eviction succeeds.
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

fn main() {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn clean_lru_and_log() {
        let mut cache = Cache::new(5);
        let mut flush = |_, _| true;
        cache.put(1, 10, 2, false, &mut flush).unwrap();
        cache.put(2, 20, 2, false, &mut flush).unwrap();
        assert_eq!(cache.get(1), Some(10));
        assert_eq!(cache.get(9), None);
        assert_eq!(cache.put(3, 30, 2, false, &mut flush), Ok(vec![2]));
        assert_eq!(
            cache.snapshot().iter().map(|entry| entry.key).collect::<Vec<_>>(),
            vec![1, 3]
        );
        assert_eq!(cache.used(), 4);
    }

    #[test]
    fn dirty_flush_failure_preserves_order_and_entry() {
        let mut cache = Cache::new(8);
        let mut setup_flush = |_, _| true;
        cache.put(1, 10, 2, true, &mut setup_flush).unwrap();
        cache.put(2, 20, 2, true, &mut setup_flush).unwrap();

        let mut fail = |key, _| key != 1;
        assert_eq!(cache.flush(2, &mut fail), Err(Error::FlushFailed { key: 1 }));
        assert!(cache.snapshot()[0].dirty);
        assert_eq!(cache.flush(1, &mut |_, _| true), Ok(vec![1]));
    }

    #[test]
    fn invalid_weight_is_atomic() {
        let mut cache = Cache::new(3);
        let mut calls = 0;
        let mut flush = |_, _| {
            calls += 1;
            true
        };
        assert_eq!(cache.put(1, 10, 0, false, &mut flush), Err(Error::ZeroWeight));
        assert_eq!(cache.put(1, 10, 4, false, &mut flush), Err(Error::TooHeavy {
            weight: 4,
            capacity: 3
        }));
        assert_eq!(calls, 0);
        assert_eq!(cache.used(), 0);
        assert!(cache.events().is_empty());
    }

    #[test]
    fn resize_failure_keeps_old_capacity_and_completed_eviction() {
        let mut cache = Cache::new(6);
        let mut setup_flush = |_, _| true;
        cache.put(1, 10, 2, false, &mut setup_flush).unwrap();
        cache.put(2, 20, 2, true, &mut setup_flush).unwrap();

        let mut fail = |_, _| false;
        assert_eq!(cache.resize(1, &mut fail), Err(Error::FlushFailed { key: 2 }));
        assert_eq!(cache.capacity(), 6);
        assert_eq!(cache.used(), 2);
        assert_eq!(cache.snapshot()[0].key, 2);
        assert!(!cache.events().contains(&Event::Resize { capacity: 1 }));
    }
}