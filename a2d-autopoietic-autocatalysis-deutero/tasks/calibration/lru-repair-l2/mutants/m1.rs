use std::collections::VecDeque;

pub struct LruCache {
    capacity: usize,
    entries: VecDeque<(i32, i32)>,
}

impl LruCache {
    pub fn new(capacity: usize) -> Self {
        Self { capacity, entries: VecDeque::new() }
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn keys_lru(&self) -> Vec<i32> {
        self.entries.iter().map(|e| e.0).collect()
    }

    pub fn get(&mut self, key: i32) -> Option<i32> {
        let pos = self.entries.iter().position(|e| e.0 == key)?;
        let entry = self.entries.remove(pos).unwrap();
        self.entries.push_back(entry);
        Some(entry.1)
    }

    pub fn put(&mut self, key: i32, value: i32) -> Option<i32> {
        if self.capacity == 0 {
            return None;
        }
        let old = self.entries.iter().position(|e| e.0 == key)
            .map(|pos| self.entries.remove(pos).unwrap().1);
        if self.entries.len() == self.capacity {
            self.entries.pop_front();
        }
        self.entries.push_back((key, value));
        old
    }

    pub fn peek(&mut self, key: i32) -> Option<i32> {
        let pos = self.entries.iter().position(|e| e.0 == key)?;
        let entry = self.entries.remove(pos).unwrap();
        self.entries.push_back(entry);
        Some(entry.1)
    }

    pub fn remove(&mut self, key: i32) -> Option<i32> {
        let pos = self.entries.iter().position(|e| e.0 == key)?;
        Some(self.entries.remove(pos).unwrap().1)
    }
}
