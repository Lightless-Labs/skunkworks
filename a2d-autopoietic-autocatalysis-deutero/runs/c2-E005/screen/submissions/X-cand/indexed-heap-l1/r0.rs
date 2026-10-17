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
}

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
        let key = canonicalize_key(key)?;
        if self.indices.contains_key(&key) {
            return Err(Error::Duplicate { key });
        }

        // Rebase ages before incrementing at the numeric limit, preserving their order.
        if self.next_age == u128::MAX {
            let mut order: Vec<usize> = (0..self.entries.len()).collect();
            order.sort_by_key(|&i| self.entries[i].age);
            for (age, &index) in order.iter().enumerate() {
                self.entries[index].age = age as u128;
            }
            self.next_age = self.entries.len() as u128;
        }

        let entry = Entry {
            item: Item {
                key: key.clone(),
                priority,
                value: value.to_owned(),
            },
            age: self.next_age,
        };
        self.next_age += 1;
        self.entries.push(entry);
        let index = self.entries.len() - 1;
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

        let last = self.entries.len() - 1;
        self.swap_entries(0, last);
        let removed = self.entries.pop().expect("heap was nonempty");
        self.indices.remove(&removed.item.key);
        if !self.entries.is_empty() {
            self.sift_down(0);
        }
        Some(removed.item)
    }

    pub fn get(&self, key: &str) -> Result<Option<Item>, Error> {
        let key = canonicalize_key(key)?;
        Ok(self
            .indices
            .get(&key)
            .map(|&index| self.entries[index].item.clone()))
    }

    fn less(&self, left: usize, right: usize) -> bool {
        let a = &self.entries[left];
        let b = &self.entries[right];
        a.item
            .priority
            .cmp(&b.item.priority)
            .then_with(|| a.item.value.len().cmp(&b.item.value.len()))
            .then_with(|| a.age.cmp(&b.age))
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
        loop {
            let left = index.saturating_mul(2).saturating_add(1);
            if left >= self.entries.len() {
                break;
            }
            let right = left + 1;
            let best = if right < self.entries.len() && self.less(right, left) {
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