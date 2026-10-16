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

struct Entry {
    item: Item,
    age: u64,
}

pub struct IndexedHeap {
    entries: Vec<Entry>,
    indices: HashMap<String, usize>,
    next_age: u64,
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
        let key = canonicalize(key)?;
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
        let key = canonicalize(key)?;
        Ok(self.indices.get(&key).map(|&index| self.entries[index].item.clone()))
    }

    pub fn decrease_key(&mut self, key: &str, priority: i64) -> Result<i64, Error> {
        self.update_priority(key, priority, true)
    }

    pub fn increase_key(&mut self, key: &str, priority: i64) -> Result<i64, Error> {
        self.update_priority(key, priority, false)
    }

    pub fn replace_value(&mut self, key: &str, value: &str) -> Result<String, Error> {
        let key = canonicalize(key)?;
        let Some(&index) = self.indices.get(&key) else {
            return Err(Error::Missing { key });
        };

        let previous = std::mem::replace(&mut self.entries[index].item.value, value.to_owned());
        self.repair(index);
        Ok(previous)
    }

    pub fn remove(&mut self, key: &str) -> Result<Item, Error> {
        let key = canonicalize(key)?;
        let Some(&index) = self.indices.get(&key) else {
            return Err(Error::Missing { key });
        };
        Ok(self.remove_at(index).item)
    }

    fn update_priority(
        &mut self,
        key: &str,
        requested: i64,
        decrease: bool,
    ) -> Result<i64, Error> {
        let key = canonicalize(key)?;
        let Some(&index) = self.indices.get(&key) else {
            return Err(Error::Missing { key });
        };

        let current = self.entries[index].item.priority;
        let wrong_direction = if decrease {
            requested >= current
        } else {
            requested <= current
        };
        if wrong_direction {
            return Err(Error::WrongDirection { current, requested });
        }

        self.entries[index].item.priority = requested;
        self.repair(index);
        Ok(current)
    }

    fn less(&self, left: usize, right: usize) -> bool {
        let a = &self.entries[left];
        let b = &self.entries[right];
        a.item
            .priority
            .cmp(&b.item.priority)
            .then_with(|| a.item.value.len().cmp(&b.item.value.len()))
            .then_with(|| a.age.cmp(&b.age))
            == std::cmp::Ordering::Less
    }

    fn swap_entries(&mut self, left: usize, right: usize) {
        self.entries.swap(left, right);
        self.indices.insert(self.entries[left].item.key.clone(), left);
        self.indices.insert(self.entries[right].item.key.clone(), right);
    }

    fn sift_up(&mut self, mut index: usize) {
        while index > 0 {
            let parent = (index - 1) / 2;
            if !self.less(index, parent) {
                break;
            }
            self.swap_entries(index, parent);
            index = parent;
        }
    }

    fn sift_down(&mut self, mut index: usize) {
        let len = self.entries.len();
        loop {
            let left = index * 2 + 1;
            if left >= len {
                break;
            }
            let right = left + 1;
            let best_child = if right < len && self.less(right, left) {
                right
            } else {
                left
            };
            if !self.less(best_child, index) {
                break;
            }
            self.swap_entries(index, best_child);
            index = best_child;
        }
    }

    fn repair(&mut self, index: usize) {
        if index > 0 && self.less(index, (index - 1) / 2) {
            self.sift_up(index);
        } else {
            self.sift_down(index);
        }
    }

    fn remove_at(&mut self, index: usize) -> Entry {
        let removed = self.entries.swap_remove(index);
        self.indices.remove(&removed.item.key);

        if index < self.entries.len() {
            self.indices
                .insert(self.entries[index].item.key.clone(), index);
            self.repair(index);
        }

        removed
    }
}

fn canonicalize(input: &str) -> Result<String, Error> {
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

    let trimmed = &bytes[start..end];
    for (offset, &byte) in trimmed.iter().enumerate() {
        let valid = if offset == 0 {
            byte.is_ascii_alphabetic()
        } else {
            byte.is_ascii_alphabetic() || byte.is_ascii_digit() || byte == b'-'
        };
        if !valid {
            return Err(Error::InvalidKey { at: start + offset });
        }
    }

    Ok(input[start..end].to_ascii_lowercase())
}