use std::collections::VecDeque;

pub struct LruCache {
    capacity: usize,
    entries: VecDeque<(i32, i32)>,
}

impl LruCache {
    pub fn new(capacity: usize) -> Self {
        Self {
            capacity,
            entries: VecDeque::new(),
        }
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn keys_lru(&self) -> Vec<i32> {
        self.entries.iter().map(|entry| entry.0).collect()
    }

    pub fn get(&mut self, key: i32) -> Option<i32> {
        let pos = self.entries.iter().position(|entry| entry.0 == key)?;
        let entry = self.entries.remove(pos)?;
        let value = entry.1;
        self.entries.push_back(entry);
        Some(value)
    }

    pub fn put(&mut self, key: i32, value: i32) -> Option<i32> {
        if self.capacity == 0 {
            return None;
        }

        if let Some(pos) = self.entries.iter().position(|entry| entry.0 == key) {
            let entry = self.entries.remove(pos)?;
            let old_value = entry.1;
            self.entries.push_back((key, value));
            return Some(old_value);
        }

        if self.entries.len() == self.capacity {
            self.entries.pop_front();
        }
        self.entries.push_back((key, value));
        None
    }

    pub fn peek(&mut self, key: i32) -> Option<i32> {
        self.entries
            .iter()
            .find(|entry| entry.0 == key)
            .map(|entry| entry.1)
    }

    pub fn remove(&mut self, key: i32) -> Option<i32> {
        let pos = self.entries.iter().position(|entry| entry.0 == key)?;
        self.entries.remove(pos).map(|entry| entry.1)
    }
}