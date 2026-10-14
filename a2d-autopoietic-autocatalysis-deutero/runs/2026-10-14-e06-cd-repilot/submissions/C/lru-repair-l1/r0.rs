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
    /// Creates an empty cache with the given weighted capacity.
    pub fn new(capacity: u64) -> Self {
        Self {
            capacity,
            used: 0,
            clock: 0,
            entries: BTreeMap::new(),
            order: VecDeque::new(),
        }
    }

    /// Returns the current weighted capacity.
    pub fn capacity(&self) -> u64 {
        self.capacity
    }

    /// Returns the total resident weight.
    pub fn used(&self) -> u64 {
        self.used
    }

    /// Returns the current logical clock.
    pub fn clock(&self) -> u64 {
        self.clock
    }

    /// Clones resident entries from least recently used to most recently used.
    pub fn snapshot(&self) -> Vec<Entry> {
        self.order
            .iter()
            .filter_map(|key| self.entries.get(key).cloned())
            .collect()
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
        // BTreeMap iteration provides ascending numeric key order.
        let expired: Vec<u64> = self
            .entries
            .iter()
            .filter(|(_, entry)| entry.expires_at.is_some_and(|deadline| deadline <= now))
            .map(|(key, _)| *key)
            .collect();

        for key in &expired {
            self.erase(*key);
        }
        self.clock = now;
        expired
    }

    /// Purges expired entries at `now` and returns their keys in ascending order.
    pub fn advance(&mut self, now: u64) -> Result<Vec<u64>, Error> {
        self.validate_time(now)?;
        Ok(self.expire(now))
    }

    /// Looks up a key at `now`, refreshing its recency on a hit.
    pub fn get(&mut self, key: u64, now: u64) -> Result<Option<i64>, Error> {
        self.validate_time(now)?;
        self.expire(now);

        let value = self.entries.get(&key).map(|entry| entry.value);
        if value.is_some() {
            self.touch(key);
        }
        Ok(value)
    }

    /// Removes a key at `now`, returning its value if it was resident.
    pub fn remove(&mut self, key: u64, now: u64) -> Result<Option<i64>, Error> {
        self.validate_time(now)?;
        self.expire(now);
        Ok(self.erase(key).map(|entry| entry.value))
    }

    /// Inserts or replaces an entry, returning purged keys followed by eviction victims.
    pub fn insert(
        &mut self,
        key: u64,
        value: i64,
        weight: u64,
        ttl: Option<u64>,
        now: u64,
    ) -> Result<Vec<u64>, Error> {
        self.validate_time(now)?;

        if weight == 0 {
            return Err(Error::ZeroWeight);
        }
        if weight > self.capacity {
            return Err(Error::TooHeavy {
                weight,
                capacity: self.capacity,
            });
        }

        let expires_at = match ttl {
            None => None,
            Some(duration) => Some(now.checked_add(duration).ok_or(Error::DeadlineOverflow)?),
        };

        let mut removed = self.expire(now);
        self.erase(key);

        if ttl == Some(0) {
            return Ok(removed);
        }

        // Subtraction is safe because weight <= capacity. This form avoids
        // overflowing when adding the proposed weight to current usage.
        let target_used = self.capacity - weight;
        while self.used > target_used {
            if let Some(victim) = self.order.front().copied() {
                self.erase(victim);
                removed.push(victim);
            } else {
                break;
            }
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

    /// Changes capacity at `now`, purging expired entries before evicting to fit.
    pub fn resize(&mut self, capacity: u64, now: u64) -> Result<Vec<u64>, Error> {
        self.validate_time(now)?;
        let mut removed = self.expire(now);
        self.capacity = capacity;

        while self.used > capacity {
            if let Some(victim) = self.order.front().copied() {
                self.erase(victim);
                removed.push(victim);
            } else {
                break;
            }
        }

        Ok(removed)
    }
}

fn main() {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn weighted_lru_insert_and_get() {
        let mut cache = Cache::new(5);
        cache.insert(1, 10, 2, None, 1).unwrap();
        cache.insert(2, 20, 2, None, 2).unwrap();
        assert_eq!(cache.get(1, 3), Ok(Some(10)));
        assert_eq!(cache.insert(3, 30, 2, None, 4), Ok(vec![2]));
        assert_eq!(
            cache.snapshot().iter().map(|entry| entry.key).collect::<Vec<_>>(),
            vec![1, 3]
        );
    }

    #[test]
    fn expiry_is_inclusive_and_ordered_by_key() {
        let mut cache = Cache::new(10);
        cache.insert(8, 80, 1, Some(2), 1).unwrap();
        cache.insert(3, 30, 1, Some(2), 1).unwrap();
        assert_eq!(cache.advance(3), Ok(vec![3, 8]));
        assert_eq!(cache.used(), 0);
    }

    #[test]
    fn invalid_insert_is_atomic_and_zero_ttl_removes_existing() {
        let mut cache = Cache::new(4);
        cache.insert(1, 10, 2, None, 1).unwrap();
        let before = cache.snapshot();

        assert_eq!(cache.insert(2, 20, 0, None, 2), Err(Error::ZeroWeight));
        assert_eq!(cache.clock(), 1);
        assert_eq!(cache.snapshot(), before);

        assert_eq!(cache.insert(1, 99, 1, Some(0), 2), Ok(vec![]));
        assert!(cache.snapshot().is_empty());
        assert_eq!(cache.used(), 0);
    }

    #[test]
    fn full_range_weights_and_deadline_overflow() {
        let mut cache = Cache::new(u64::MAX);
        cache.insert(1, 1, u64::MAX, None, 0).unwrap();
        assert_eq!(
            cache.insert(2, 2, 1, None, 1),
            Ok(vec![1])
        );
        assert_eq!(cache.used(), 1);

        assert_eq!(
            cache.insert(3, 3, 1, Some(1), u64::MAX),
            Err(Error::DeadlineOverflow)
        );
        assert_eq!(cache.clock(), 1);
    }
}