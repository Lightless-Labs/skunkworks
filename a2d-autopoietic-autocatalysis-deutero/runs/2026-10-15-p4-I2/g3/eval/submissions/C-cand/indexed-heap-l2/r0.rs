use std::collections::HashMap;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Item {
    pub key: String,
    pub priority: i64,
    pub value: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Error {
    EmptyKey,
    InvalidKey { at: usize },
    Duplicate { key: String },
    Missing { key: String },
    WrongDirection { current: i64, requested: i64 },
}

#[derive(Debug, Clone)]
struct Entry {
    item: Item,
    age: u128,
}

pub struct IndexedHeap {
    entries: Vec<Entry>,
    indices: HashMap<String, usize>,
    next_age: u128,
}

impl IndexedHeap {
    pub fn new() -> Self {
        Self {
            entries: Vec::new(),
            indices: HashMap::new(),
            next_age: 0,
        }
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    pub fn push(&mut self, key: &str, priority: i64, value: &str) -> Result<(), Error> {
        let key = canonical_key(key)?;
        if self.indices.contains_key(&key) {
            return Err(Error::Duplicate { key });
        }

        let age = self.next_age;
        self.next_age = self.next_age.wrapping_add(1);
        let index = self.entries.len();
        self.entries.push(Entry {
            item: Item {
                key: key.clone(),
                priority,
                value: value.to_owned(),
            },
            age,
        });
        self.indices.insert(key, index);
        self.sift_up(index);
        Ok(())
    }

    pub fn peek(&self) -> Option<Item> {
        self.entries.first().map(|entry| entry.item.clone())
    }

    pub fn pop(&mut self) -> Option<Item> {
        if self.entries.is_empty() {
            return None;
        }
        Some(self.remove_at(0).item)
    }

    pub fn get(&self, key: &str) -> Result<Option<Item>, Error> {
        let key = canonical_key(key)?;
        Ok(self
            .indices
            .get(&key)
            .map(|&index| self.entries[index].item.clone()))
    }

    pub fn decrease_key(&mut self, key: &str, priority: i64) -> Result<i64, Error> {
        let key = canonical_key(key)?;
        let index = *self
            .indices
            .get(&key)
            .ok_or_else(|| Error::Missing { key: key.clone() })?;
        let current = self.entries[index].item.priority;
        if priority >= current {
            return Err(Error::WrongDirection {
                current,
                requested: priority,
            });
        }

        self.entries[index].item.priority = priority;
        self.sift_up(index);
        Ok(current)
    }

    pub fn increase_key(&mut self, key: &str, priority: i64) -> Result<i64, Error> {
        let key = canonical_key(key)?;
        let index = *self
            .indices
            .get(&key)
            .ok_or_else(|| Error::Missing { key: key.clone() })?;
        let current = self.entries[index].item.priority;
        if priority <= current {
            return Err(Error::WrongDirection {
                current,
                requested: priority,
            });
        }

        self.entries[index].item.priority = priority;
        self.sift_down(index);
        Ok(current)
    }

    pub fn replace_value(&mut self, key: &str, value: &str) -> Result<String, Error> {
        let key = canonical_key(key)?;
        let index = *self
            .indices
            .get(&key)
            .ok_or_else(|| Error::Missing { key: key.clone() })?;

        let previous = std::mem::replace(
            &mut self.entries[index].item.value,
            value.to_owned(),
        );
        self.repair(index);
        Ok(previous)
    }

    pub fn remove(&mut self, key: &str) -> Result<Item, Error> {
        let key = canonical_key(key)?;
        let index = *self
            .indices
            .get(&key)
            .ok_or_else(|| Error::Missing { key: key.clone() })?;
        Ok(self.remove_at(index).item)
    }

    fn before(&self, a: usize, b: usize) -> bool {
        let a = &self.entries[a];
        let b = &self.entries[b];
        (a.item.priority, a.item.value.len(), a.age)
            < (b.item.priority, b.item.value.len(), b.age)
    }

    fn swap_slots(&mut self, a: usize, b: usize) {
        self.entries.swap(a, b);
        self.indices.insert(self.entries[a].item.key.clone(), a);
        self.indices.insert(self.entries[b].item.key.clone(), b);
    }

    fn sift_up(&mut self, mut index: usize) {
        while index > 0 {
            let parent = (index - 1) / 2;
            if !self.before(index, parent) {
                break;
            }
            self.swap_slots(index, parent);
            index = parent;
        }
    }

    fn sift_down(&mut self, mut index: usize) {
        loop {
            let left = index * 2 + 1;
            if left >= self.entries.len() {
                break;
            }
            let right = left + 1;
            let best = if right < self.entries.len() && self.before(right, left) {
                right
            } else {
                left
            };
            if !self.before(best, index) {
                break;
            }
            self.swap_slots(index, best);
            index = best;
        }
    }

    fn repair(&mut self, index: usize) {
        if index > 0 && self.before(index, (index - 1) / 2) {
            self.sift_up(index);
        } else {
            self.sift_down(index);
        }
    }

    fn remove_at(&mut self, index: usize) -> Entry {
        let last = self.entries.len() - 1;
        self.swap_slots(index, last);
        let removed = self.entries.pop().expect("index is in bounds");
        self.indices.remove(&removed.item.key);

        if index < self.entries.len() {
            self.repair(index);
        }
        removed
    }
}

fn canonical_key(input: &str) -> Result<String, Error> {
    let bytes = input.as_bytes();
    let mut start = 0;
    let mut end = bytes.len();

    while start < end && bytes[start] == b' ' {
        start += 1;
    }
    while end > start && bytes[end - 1] == b' ' {
        end -= 1;
    }

    if start == end {
        return Err(Error::EmptyKey);
    }

    for (offset, &byte) in bytes[start..end].iter().enumerate() {
        let valid = if offset == 0 {
            byte.is_ascii_alphabetic()
        } else {
            byte.is_ascii_alphanumeric() || byte == b'-'
        };
        if !valid {
            return Err(Error::InvalidKey { at: start + offset });
        }
    }

    Ok(input[start..end].to_ascii_lowercase())
}

fn main() {}