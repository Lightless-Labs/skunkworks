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
    /// Creates an empty weighted cache with the supplied capacity.
    pub fn new(capacity: u64) -> Self {
        Self {
            capacity,
            used: 0,
            clock: 0,
            entries: BTreeMap::new(),
            order: VecDeque::new(),
        }
    }

    /// Returns the current weight capacity.
    pub fn capacity(&self) -> u64 {
        self.capacity
    }

    /// Returns the total weight of resident entries.
    pub fn used(&self) -> u64 {
        self.used
    }

    /// Returns the current logical clock.
    pub fn clock(&self) -> u64 {
        self.clock
    }

    /// Returns cloned resident entries from least to most recently used.
    pub fn snapshot(&self) -> Vec<Entry> {
        self.order
            .iter()
            .filter_map(|key| self.entries.get(key).cloned())
            .collect()
    }

    fn make_deadline(ttl: Option<u64>, now: u64) -> Result<Option<u64>, Error> {
        match ttl {
            None => Ok(None),
            Some(duration) => now
                .checked_add(duration)
                .map(Some)
                .ok_or(Error::DeadlineOverflow),
        }
    }

    fn validate_time(&self, now: u64) -> Result<(), Error> {
        if now < self.clock {
            return Err(Error::BackwardTime {
                last: self.clock,
                now,
            });
        }
        Ok(())
    }

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

    fn erase(&mut self, key: u64) -> Option<Entry> {
        let entry = self.entries.remove(&key)?;
        self.used -= entry.weight;
        self.order.retain(|existing| *existing != key);
        Some(entry)
    }

    fn touch(&mut self, key: u64) {
        self.order.retain(|existing| *existing != key);
        self.order.push_back(key);
    }

    fn expire(&mut self, now: u64) -> Vec<u64> {
        // BTreeMap iteration makes the returned expiration keys ascending.
        let expired: Vec<u64> = self
            .entries
            .iter()
            .filter_map(|(&key, entry)| {
                entry
                    .expires_at
                    .is_some_and(|deadline| deadline <= now)
                    .then_some(key)
            })
            .collect();

        for &key in &expired {
            self.erase(key);
        }
        self.clock = now;
        expired
    }

    /// Purges expired entries at `now` and returns their keys in ascending order.
    pub fn advance(&mut self, now: u64) -> Result<Vec<u64>, Error> {
        self.validate_time(now)?;
        Ok(self.expire(now))
    }

    /// Looks up a key at `now`, making a hit the most recently used entry.
    pub fn get(&mut self, key: u64, now: u64) -> Result<Option<i64>, Error> {
        self.validate_time(now)?;
        self.expire(now);

        let value = self.entries.get(&key).map(|entry| entry.value);
        if value.is_some() {
            self.touch(key);
        }
        Ok(value)
    }

    /// Removes a surviving key at `now` and returns its value, if present.
    pub fn remove(&mut self, key: u64, now: u64) -> Result<Option<i64>, Error> {
        self.validate_time(now)?;
        self.expire(now);
        Ok(self.erase(key).map(|entry| entry.value))
    }

    /// Inserts or replaces an entry and returns purged keys followed by eviction victims.
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

        while self.used > self.capacity - weight {
            let victim = self
                .order
                .front()
                .copied()
                .expect("positive used weight implies a resident entry");
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

    /// Changes capacity at `now`, returning purged keys followed by eviction victims.
    pub fn resize(&mut self, capacity: u64, now: u64) -> Result<Vec<u64>, Error> {
        self.validate_time(now)?;

        let mut removed = self.expire(now);
        self.capacity = capacity;
        while self.used > capacity {
            let key = self
                .order
                .front()
                .copied()
                .expect("positive used weight implies a resident entry");
            self.erase(key);
            removed.push(key);
        }
        Ok(removed)
    }
}

fn main() {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn weighted_lru_eviction() {
        let mut cache = Cache::new(5);
        cache.insert(1, 10, 2, None, 1).unwrap();
        cache.insert(2, 20, 2, None, 2).unwrap();
        assert_eq!(cache.get(1, 3), Ok(Some(10)));
        assert_eq!(cache.insert(3, 30, 2, None, 4), Ok(vec![2]));
        assert_eq!(
            cache.snapshot().iter().map(|entry| entry.key).collect::<Vec<_>>(),
            vec![1, 3]
        );
        assert_eq!(cache.used(), 4);
    }

    #[test]
    fn expiry_uses_inclusive_deadline_and_sorted_keys() {
        let mut cache = Cache::new(10);
        cache.insert(8, 80, 1, Some(2), 1).unwrap();
        cache.insert(3, 30, 1, Some(2), 1).unwrap();
        assert_eq!(cache.advance(3), Ok(vec![3, 8]));
        assert_eq!(cache.used(), 0);
    }

    #[test]
    fn validation_is_atomic_and_zero_ttl_removes_without_eviction() {
        let mut cache = Cache::new(3);
        cache.insert(1, 11, 2, Some(2), 1).unwrap();
        let before = cache.snapshot();

        assert_eq!(
            cache.insert(2, 22, 0, None, 3),
            Err(Error::ZeroWeight)
        );
        assert_eq!(cache.clock(), 1);
        assert_eq!(cache.snapshot(), before);

        assert_eq!(cache.insert(1, 99, 1, Some(0), 1), Ok(vec![]));
        assert!(cache.snapshot().is_empty());
        assert_eq!(cache.used(), 0);
    }

    #[test]
    fn end_to_end_replace_resize_and_remove() {
        let mut cache = Cache::new(7);
        cache.insert(1, 5, 2, None, 0).unwrap();
        cache.insert(2, 8, 3, None, 1).unwrap();
        cache.insert(1, 9, 4, None, 1).unwrap();
        assert_eq!(cache.snapshot().iter().map(|entry| entry.key).collect::<Vec<_>>(), vec![2, 1]);
        assert_eq!(cache.resize(3, 2), Ok(vec![2, 1]));
        assert_eq!(cache.remove(1, 3), Ok(None));
        assert_eq!(cache.used(), 0);
    }
}