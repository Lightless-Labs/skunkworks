use std::collections::VecDeque;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PutError {
    TooHeavy { weight: usize, capacity: usize },
}

struct Entry {
    key: i32,
    value: i32,
    weight: usize,
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

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn total_weight(&self) -> usize {
        self.total_weight
    }

    pub fn keys_lru(&self) -> Vec<i32> {
        self.entries.iter().map(|e| e.key).collect()
    }

    pub fn get(&mut self, key: i32) -> Option<i32> {
        let pos = self.entries.iter().position(|e| e.key == key)?;
        let entry = self.entries.remove(pos).unwrap();
        let value = entry.value;
        self.entries.push_back(entry);
        Some(value)
    }

    pub fn peek(&mut self, key: i32) -> Option<i32> {
        self.entries.iter().find(|e| e.key == key).map(|e| e.value)
    }

    pub fn remove(&mut self, key: i32) -> Option<i32> {
        let pos = self.entries.iter().position(|e| e.key == key)?;
        let entry = self.entries.remove(pos).unwrap();
        self.total_weight -= entry.weight;
        Some(entry.value)
    }

    pub fn put(&mut self, key: i32, value: i32, weight: usize) -> Result<Option<i32>, PutError> {
        if weight > self.capacity {
            self.remove(key);
            return Err(PutError::TooHeavy { weight, capacity: self.capacity });
        }
        let old = self.remove(key);
        while self.total_weight > self.capacity - weight {
            let victim = self.entries.pop_front().unwrap();
            self.total_weight -= victim.weight;
        }
        self.entries.push_back(Entry { key, value, weight });
        self.total_weight += weight;
        Ok(old)
    }
}
