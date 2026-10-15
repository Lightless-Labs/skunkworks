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
    pub fn new(capacity: usize, period: u32) -> Result<Self, Error> {
        if period == 0 {
            return Err(Error::ZeroPeriod);
        }
        Ok(Self {
            capacity,
            period,
            pending: 0,
            entries: BTreeMap::new(),
            recency: VecDeque::new(),
        })
    }

    pub fn capacity(&self) -> usize {
        self.capacity
    }

    pub fn pending(&self) -> u32 {
        self.pending
    }

    pub fn snapshot(&self) -> Vec<Entry> {
        let mut result = self.entries_by_recency();
        result.sort_by_key(|entry| entry.frequency);
        result
    }

    fn entries_by_recency(&self) -> Vec<Entry> {
        self.recency
            .iter()
            .filter_map(|key| self.entries.get(key).cloned())
            .collect()
    }

    fn halve(&mut self) {
        for entry in self.entries.values_mut() {
            entry.frequency = (entry.frequency + 1) / 2;
        }
    }

    fn begin(&mut self) {
        self.pending += 1;
        if self.pending == self.period {
            self.halve();
            self.pending = 0;
        }
    }

    fn touch(&mut self, key: u64) {
        self.recency.retain(|&k| k != key);
        self.recency.push_back(key);
    }

    fn credit(&mut self, key: u64) {
        if let Some(entry) = self.entries.get_mut(&key) {
            entry.frequency = entry.frequency.saturating_add(1).min(7);
        }
        self.touch(key);
    }

    fn erase(&mut self, key: u64) -> Option<Entry> {
        let entry = self.entries.remove(&key)?;
        self.recency.retain(|&k| k != key);
        Some(entry)
    }

    fn victim(&self) -> Option<u64> {
        self.recency.iter().find_map(|key| {
            let frequency = self.entries.get(key)?.frequency;
            let is_lower = self.recency.iter().take_while(|&&k| k != *key).all(|&earlier| {
                self.entries
                    .get(&earlier)
                    .is_none_or(|entry| entry.frequency >= frequency)
            });
            if is_lower { Some(*key) } else { None }
        }).or_else(|| {
            self.recency.iter().copied().min_by_key(|key| {
                self.entries.get(key).map_or(u8::MAX, |entry| entry.frequency)
            })
        })
    }

    fn take_victim(&mut self) -> u64 {
        let key = self.victim().expect("victim exists when cache is full");
        self.erase(key);
        key
    }

    pub fn get(&mut self, key: u64) -> Option<i64> {
        self.begin();
        let value = self.entries.get(&key).map(|entry| entry.value);
        if value.is_some() {
            self.credit(key);
        }
        value
    }

    pub fn put(&mut self, key: u64, value: i64) -> Result<Vec<u64>, Error> {
        if self.capacity == 0 {
            return Err(Error::Disabled);
        }
        self.begin();

        if let Some(entry) = self.entries.get_mut(&key) {
            entry.value = value;
            self.credit(key);
            return Ok(Vec::new());
        }

        let mut removed = Vec::new();
        if self.entries.len() == self.capacity {
            removed.push(self.take_victim());
        }
        self.entries.insert(
            key,
            Entry {
                key,
                value,
                frequency: 1,
            },
        );
        self.touch(key);
        Ok(removed)
    }

    pub fn remove(&mut self, key: u64) -> Option<i64> {
        self.begin();
        self.erase(key).map(|entry| entry.value)
    }

    pub fn age(&mut self, rounds: u8) {
        for _ in 0..rounds {
            self.halve();
        }
    }

    pub fn resize(&mut self, capacity: usize) -> Vec<u64> {
        self.capacity = capacity;
        let mut removed = Vec::new();
        while self.entries.len() > capacity {
            removed.push(self.take_victim());
        }
        removed
    }
}

fn main() {}