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
    indices: std::collections::HashMap<String, usize>,
    next_age: u128,
}

impl IndexedHeap {
    pub fn new() -> Self {
        Self {
            entries: Vec::new(),
            indices: std::collections::HashMap::new(),
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
        let key = canonicalize_key(key)?;
        if self.indices.contains_key(&key) {
            return Err(Error::Duplicate { key });
        }

        let age = self.next_age;
        let next_age = age.checked_add(1).unwrap_or_else(|| {
            // Rebase retained ages while preserving their relative order.
            // This branch is practically unreachable, but keeps insertion total.
            0
        });

        if self.next_age == u128::MAX {
            self.rebase_ages();
        }

        let age = self.next_age;
        self.next_age = self.next_age.saturating_add(1);

        self.entries.push(Entry {
            item: Item {
                key: key.clone(),
                priority,
                value: value.to_owned(),
            },
            age,
        });
        let index = self.entries.len() - 1;
        self.indices.insert(key, index);
        self.sift_up(index);

        let _ = next_age;
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
        let key = canonicalize_key(key)?;
        Ok(self.indices.get(&key).map(|&index| self.entries[index].item.clone()))
    }

    pub fn decrease_key(&mut self, key: &str, priority: i64) -> Result<i64, Error> {
        let key = canonicalize_key(key)?;
        let index = *self.indices.get(&key).ok_or_else(|| Error::Missing { key: key.clone() })?;
        let current = self.entries[index].item.priority;
        if priority >= current {
            return Err(Error::WrongDirection {
                current,
                requested: priority,
            });
        }
        self.entries[index].item.priority = priority;
        self.repair(index);
        Ok(current)
    }

    pub fn increase_key(&mut self, key: &str, priority: i64) -> Result<i64, Error> {
        let key = canonicalize_key(key)?;
        let index = *self.indices.get(&key).ok_or_else(|| Error::Missing { key: key.clone() })?;
        let current = self.entries[index].item.priority;
        if priority <= current {
            return Err(Error::WrongDirection {
                current,
                requested: priority,
            });
        }
        self.entries[index].item.priority = priority;
        self.repair(index);
        Ok(current)
    }

    pub fn replace_value(&mut self, key: &str, value: &str) -> Result<String, Error> {
        let key = canonicalize_key(key)?;
        let index = *self.indices.get(&key).ok_or_else(|| Error::Missing { key: key.clone() })?;
        let previous = std::mem::replace(&mut self.entries[index].item.value, value.to_owned());
        self.repair(index);
        Ok(previous)
    }

    pub fn remove(&mut self, key: &str) -> Result<Item, Error> {
        let key = canonicalize_key(key)?;
        let index = *self.indices.get(&key).ok_or_else(|| Error::Missing { key: key.clone() })?;
        Ok(self.remove_at(index).item)
    }

    fn less(&self, a: usize, b: usize) -> bool {
        let left = &self.entries[a];
        let right = &self.entries[b];
        left.item.priority
            .cmp(&right.item.priority)
            .then_with(|| left.item.value.len().cmp(&right.item.value.len()))
            .then_with(|| left.age.cmp(&right.age))
            .is_lt()
    }

    fn swap_entries(&mut self, a: usize, b: usize) {
        self.entries.swap(a, b);
        let key_a = self.entries[a].item.key.clone();
        let key_b = self.entries[b].item.key.clone();
        self.indices.insert(key_a, a);
        self.indices.insert(key_b, b);
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
            let left = index.saturating_mul(2).saturating_add(1);
            if left >= len {
                break;
            }
            let right = left + 1;
            let best = if right < len && self.less(right, left) {
                right
            } else {
                left
            };
            if !self.less(best, index) {
                break;
            }
            self.swap_entries(index, best);
            index = best;
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
        let removed_key = self.entries[index].item.key.clone();
        let last = self.entries.len() - 1;
        self.entries.swap(index, last);
        let removed = self.entries.pop().expect("index is valid");
        self.indices.remove(&removed_key);

        if index < self.entries.len() {
            let moved_key = self.entries[index].item.key.clone();
            self.indices.insert(moved_key, index);
            self.repair(index);
        }
        removed
    }

    fn rebase_ages(&mut self) {
        let mut order: Vec<usize> = (0..self.entries.len()).collect();
        order.sort_by_key(|&index| self.entries[index].age);
        for (age, index) in order.into_iter().enumerate() {
            self.entries[index].age = age as u128;
        }
        self.next_age = self.entries.len() as u128;
    }
}

fn canonicalize_key(input: &str) -> Result<String, Error> {
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