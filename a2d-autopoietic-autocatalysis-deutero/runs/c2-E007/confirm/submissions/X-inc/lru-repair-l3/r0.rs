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

    pub fn capacities(&self) -> Vec<u64> {
        self.capacities.clone()
    }

    pub fn usage(&self) -> Vec<u64> {
        self.usage.clone()
    }

    pub fn used(&self) -> u128 {
        self.usage.iter().map(|&n| u128::from(n)).sum()
    }

    pub fn snapshot(&self) -> Vec<Entry> {
        self.order
            .iter()
            .filter_map(|key| self.entries.get(key).cloned())
            .collect()
    }

    fn shard(&self, key: u64) -> usize {
        (key % self.capacities.len() as u64) as usize
    }

    fn touch(&mut self, key: u64) {
        self.order.retain(|&k| k != key);
        self.order.push_back(key);
    }

    fn erase(&mut self, key: u64) -> Option<Entry> {
        let entry = self.entries.remove(&key)?;
        self.usage[entry.shard] -= entry.weight;
        self.order.retain(|&k| k != key);
        Some(entry)
    }

    fn local_victim(&self, shard: usize) -> Option<u64> {
        self.order
            .iter()
            .find(|&&key| {
                self.entries
                    .get(&key)
                    .is_some_and(|e| e.shard == shard && !e.pinned)
            })
            .copied()
    }

    fn global_victim(&self) -> Option<u64> {
        self.order
            .iter()
            .find(|&&key| self.entries.get(&key).is_some_and(|e| !e.pinned))
            .copied()
    }

    fn pinned_weight(&self, shard: Option<usize>, exclude: Option<u64>) -> u128 {
        self.entries
            .values()
            .filter(|e| {
                e.pinned
                    && Some(e.key) != exclude
                    && shard.is_none_or(|s| e.shard == s)
            })
            .map(|e| u128::from(e.weight))
            .sum()
    }

    pub fn get(&mut self, key: u64) -> Option<i64> {
        let value = self.entries.get(&key).map(|e| e.value);
        if value.is_some() {
            self.touch(key);
        }
        value
    }

    pub fn set_pin(&mut self, key: u64, pinned: bool) -> Result<(), Error> {
        let entry = self.entries.get_mut(&key).ok_or(Error::Missing { key })?;
        entry.pinned = pinned;
        Ok(())
    }

    pub fn remove(&mut self, key: u64) -> Result<i64, Error> {
        let entry = self.entries.get(&key).ok_or(Error::Missing { key })?;
        if entry.pinned {
            return Err(Error::Pinned { key });
        }
        Ok(self.erase(key).expect("entry checked above").value)
    }

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

        let fixed = self.pinned_weight(Some(shard), Some(key));
        if fixed > u128::from(capacity - weight) {
            return Err(Error::Blocked { shard });
        }

        if self.entries.contains_key(&key) {
            self.erase(key);
        }

        let mut removed = Vec::new();
        while self.usage[shard] > capacity - weight {
            let victim = self
                .local_victim(shard)
                .expect("feasibility check guarantees an evictable entry");
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

    pub fn resize(&mut self, shard: usize, capacity: u64) -> Result<Vec<u64>, Error> {
        if shard >= self.capacities.len() {
            return Err(Error::BadShard { shard });
        }

        let minimum = self.pinned_weight(Some(shard), None);
        if minimum > u128::from(capacity) {
            return Err(Error::PinnedCapacity {
                minimum,
                target: u128::from(capacity),
            });
        }

        self.capacities[shard] = capacity;
        let mut removed = Vec::new();
        while self.usage[shard] > capacity {
            let victim = self
                .local_victim(shard)
                .expect("pinned weight fits within requested capacity");
            self.erase(victim);
            removed.push(victim);
        }
        Ok(removed)
    }

    pub fn trim(&mut self, target: u128) -> Result<Vec<u64>, Error> {
        let minimum = self.pinned_weight(None, None);
        if minimum > target {
            return Err(Error::PinnedCapacity { minimum, target });
        }

        let mut removed = Vec::new();
        while self.used() > target {
            let victim = self
                .global_victim()
                .expect("pinned weight fits within target");
            self.erase(victim);
            removed.push(victim);
        }
        Ok(removed)
    }
}