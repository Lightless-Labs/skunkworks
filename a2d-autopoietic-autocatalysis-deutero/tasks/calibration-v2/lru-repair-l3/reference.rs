use std::collections::{BTreeMap, VecDeque};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Error {
    EmptyShards,
    ZeroWeight,
    TooHeavy { weight: u64, capacity: u64 },
    Blocked { shard: usize },
    Missing { key: u64 },
    Pinned { key: u64 },
    BadShard { shard: usize },
    PinnedCapacity { minimum: u128, target: u128 },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Entry {
    pub key: u64,
    pub value: i64,
    pub weight: u64,
    pub pinned: bool,
    pub shard: usize,
}

#[derive(Debug, Clone)]
pub struct Cache {
    capacities: Vec<u64>,
    usage: Vec<u64>,
    entries: BTreeMap<u64, Entry>,
    order: VecDeque<u64>,
}
impl Cache {
    /// Creates a cache with caller-supplied shard budgets.
    /// The capacity vector also defines the shard count.
    /// The resulting cache owns all shard storage.
    ///
    /// The complete contract is defined by the task requirements.
    pub fn new(capacities: Vec<u64>) -> Result<Self, Error> {
        if capacities.is_empty() {
            return Err(Error::EmptyShards);
        }
        Ok(Self {
            usage: vec![0; capacities.len()],
            capacities,
            entries: BTreeMap::new(),
            order: VecDeque::new(),
        })
    }

    /// Copies the shard capacity settings.
    /// The vector is ordered by shard index.
    /// The caller owns the returned copy.
    ///
    /// The complete contract is defined by the task requirements.
    pub fn capacities(&self) -> Vec<u64> {
        self.capacities.clone()
    }

    /// Copies the resident weight of each shard.
    /// The vector is ordered by shard index.
    /// The caller owns the returned copy.
    ///
    /// The complete contract is defined by the task requirements.
    pub fn usage(&self) -> Vec<u64> {
        self.usage.clone()
    }

    /// Reads total resident weight across all shards.
    /// The return type accommodates aggregate weights.
    /// Individual shard accounting is available through usage.
    ///
    /// The complete contract is defined by the task requirements.
    pub fn used(&self) -> u128 {
        self.usage.iter().map(|n| *n as u128).sum()
    }

    /// Copies resident entries in global cache order.
    /// Entries include their shard and pin metadata.
    /// The caller owns the returned vector.
    ///
    /// The complete contract is defined by the task requirements.
    pub fn snapshot(&self) -> Vec<Entry> {
        self.order.iter().map(|k| self.entries[k].clone()).collect()
    }

    // Maps a key into the configured shard collection.
    fn shard(&self, key: u64) -> usize {
        (key % self.capacities.len() as u64) as usize
    }

    // Records a key in the cache recency structure.
    fn touch(&mut self, key: u64) {
        self.order.retain(|k| *k != key);
        self.order.push_back(key);
    }

    // Removes the stored entry and its accounting metadata.
    fn erase(&mut self, key: u64) -> Option<Entry> {
        let entry = self.entries.remove(&key)?;
        self.usage[entry.shard] -= entry.weight;
        self.order.retain(|k| *k != key);
        Some(entry)
    }

    // Selects an eligible victim inside a specified shard.
    fn local_victim(&self, shard: usize) -> Option<u64> {
        self.order
            .iter()
            .find(|k| self.entries[k].shard == shard && !self.entries[k].pinned)
            .copied()
    }

    // Selects an eligible victim across the shard collection.
    fn global_victim(&self) -> Option<u64> {
        self.order.iter().find(|k| !self.entries[k].pinned).copied()
    }

    // Collects protected resident weight for a shard selection.
    fn pinned_weight(&self, shard: Option<usize>) -> u128 {
        self.entries
            .values()
            .filter(|e| e.pinned && shard.is_none_or(|s| e.shard == s))
            .map(|e| e.weight as u128)
            .sum()
    }

    /// Looks up the value associated with a key.
    /// Routing follows the cache shard configuration.
    /// An absent key returns an empty option.
    ///
    /// The complete contract is defined by the task requirements.
    pub fn get(&mut self, key: u64) -> Option<i64> {
        let value = self.entries.get(&key).map(|e| e.value);
        if value.is_some() {
            self.touch(key);
        }
        value
    }

    /// Changes residency protection for a key.
    /// The supplied boolean becomes its pin setting.
    /// The result reports whether the key exists.
    ///
    /// The complete contract is defined by the task requirements.
    pub fn set_pin(&mut self, key: u64, pinned: bool) -> Result<(), Error> {
        let e = self.entries.get_mut(&key).ok_or(Error::Missing { key })?;
        e.pinned = pinned;
        Ok(())
    }

    /// Requests explicit deletion of a key.
    /// On success the stored value is returned.
    /// The result can report residency protection errors.
    ///
    /// The complete contract is defined by the task requirements.
    pub fn remove(&mut self, key: u64) -> Result<i64, Error> {
        let e = self.entries.get(&key).ok_or(Error::Missing { key })?;
        if e.pinned {
            return Err(Error::Pinned { key });
        }
        Ok(self.erase(key).unwrap().value)
    }

    /// Stores a value with weight and pin metadata.
    /// The key selects the destination shard.
    /// The returned list reports removals performed for storage.
    ///
    /// The complete contract is defined by the task requirements.
    pub fn insert(
        &mut self,
        key: u64,
        value: i64,
        weight: u64,
        pinned: bool,
    ) -> Result<Vec<u64>, Error> {
        let shard = self.shard(key);
        let capacity = self.capacities[shard];
        if weight == 0 {
            return Err(Error::ZeroWeight);
        }
        if weight > capacity {
            return Err(Error::TooHeavy { weight, capacity });
        }
        let fixed: u64 = self
            .entries
            .values()
            .filter(|e| e.shard == shard && e.pinned && e.key != key)
            .map(|e| e.weight)
            .sum();
        if fixed > capacity - weight {
            return Err(Error::Blocked { shard });
        }
        self.erase(key);
        let mut removed = Vec::new();
        while self.usage[shard] > capacity - weight {
            let victim = self.local_victim(shard).unwrap();
            self.erase(victim);
            removed.push(victim);
        }
        self.usage[shard] += weight;
        self.entries.insert(
            key,
            Entry {
                key,
                value,
                weight,
                pinned,
                shard,
            },
        );
        self.touch(key);
        Ok(removed)
    }

    /// Changes the capacity of a specified shard.
    /// The result reports removals performed for that change.
    /// The shard is identified by its vector index.
    ///
    /// The complete contract is defined by the task requirements.
    pub fn resize(&mut self, shard: usize, capacity: u64) -> Result<Vec<u64>, Error> {
        if shard >= self.capacities.len() {
            return Err(Error::BadShard { shard });
        }
        let minimum = self.pinned_weight(Some(shard));
        if minimum > capacity as u128 {
            return Err(Error::PinnedCapacity {
                minimum,
                target: capacity as u128,
            });
        }
        self.capacities[shard] = capacity;
        let mut removed = Vec::new();
        while self.usage[shard] > capacity {
            let victim = self.local_victim(shard).unwrap();
            self.erase(victim);
            removed.push(victim);
        }
        Ok(removed)
    }

    /// Reduces aggregate resident weight.
    /// The target applies across the shard collection.
    /// The result reports removals performed for that target.
    ///
    /// The complete contract is defined by the task requirements.
    pub fn trim(&mut self, target: u128) -> Result<Vec<u64>, Error> {
        let minimum = self.pinned_weight(None);
        if minimum > target {
            return Err(Error::PinnedCapacity { minimum, target });
        }
        let mut removed = Vec::new();
        while self.used() > target {
            let victim = self.global_victim().unwrap();
            self.erase(victim);
            removed.push(victim);
        }
        Ok(removed)
    }
}
