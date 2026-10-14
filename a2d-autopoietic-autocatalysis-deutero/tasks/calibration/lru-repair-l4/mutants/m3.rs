use std::collections::VecDeque;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PutError {
    TooHeavy { weight: usize, capacity: usize },
}

struct Entry {
    key: i32,
    value: i32,
    weight: usize,
    deadline: Option<u64>,
}

pub struct LruCache {
    capacity: usize,
    total_weight: usize,
    entries: VecDeque<Entry>,
}

impl LruCache {
    pub fn new(capacity: usize) -> Self {
        Self { capacity, total_weight: 0, entries: VecDeque::new() }
    }

    fn purge(&mut self, now: u64) {
        let total = &mut self.total_weight;
        self.entries.retain(|e| {
            let expired = e.deadline.is_some_and(|d| d <= now);
            if expired { *total -= e.weight; }
            !expired
        });
    }

    pub fn len(&mut self, now: u64) -> usize {
        self.purge(now);
        self.entries.len()
    }

    pub fn total_weight(&mut self, now: u64) -> usize {
        self.purge(now);
        self.total_weight
    }

    pub fn keys_lru(&mut self, now: u64) -> Vec<i32> {
        self.purge(now);
        self.entries.iter().map(|e| e.key).collect()
    }

    pub fn get(&mut self, key: i32, now: u64) -> Option<i32> {
        self.purge(now);
        let pos = self.entries.iter().position(|e| e.key == key)?;
        let entry = self.entries.remove(pos).unwrap();
        let value = entry.value;
        self.entries.push_back(entry);
        Some(value)
    }

    pub fn peek(&mut self, key: i32, now: u64) -> Option<i32> {
        self.purge(now);
        self.entries.iter().find(|e| e.key == key).map(|e| e.value)
    }

    pub fn remove(&mut self, key: i32, now: u64) -> Option<i32> {
        self.purge(now);
        self.remove_live(key)
    }

    fn remove_live(&mut self, key: i32) -> Option<i32> {
        let pos = self.entries.iter().position(|e| e.key == key)?;
        let entry = self.entries.remove(pos).unwrap();
        self.total_weight -= entry.weight;
        Some(entry.value)
    }

    pub fn put(&mut self, key: i32, value: i32, weight: usize, ttl: Option<u64>, now: u64)
        -> Result<Option<i32>, PutError>
    {
        if weight > self.capacity {
            return Err(PutError::TooHeavy { weight, capacity: self.capacity });
        }
        self.purge(now);
        let old = self.remove_live(key);
        let deadline = ttl.map(|t| now.saturating_add(t));
        if deadline.is_some_and(|d| d <= now) {
            return Ok(old);
        }
        while self.total_weight > self.capacity - weight {
            let victim = self.entries.pop_front().unwrap();
            self.total_weight -= victim.weight;
        }
        self.entries.push_back(Entry { key, value, weight, deadline });
        self.total_weight += weight;
        Ok(old)
    }
}
