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

#[derive(Debug)]
pub struct IndexedHeap {
    items: Vec<Entry>,
    indices: HashMap<String, usize>,
    next_age: u128,
}

#[derive(Debug)]
struct Entry {
    item: Item,
    age: u128,
}

impl IndexedHeap {
    pub fn new() -> Self {
        Self {
            items: Vec::new(),
            indices: HashMap::new(),
            next_age: 0,
        }
    }

    pub fn len(&self) -> usize {
        self.items.len()
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    pub fn push(&mut self, key: &str, priority: i64, value: &str) -> Result<(), Error> {
        let key = canonicalize_key(key)?;
        if self.indices.contains_key(&key) {
            return Err(Error::Duplicate { key });
        }

        // Renumber retained entries if the age counter ever reaches its limit.
        if self.next_age == u128::MAX {
            let mut order: Vec<usize> = (0..self.items.len()).collect();
            order.sort_by_key(|&i| self.items[i].age);
            for (age, index) in order.into_iter().enumerate() {
                self.items[index].age = age as u128;
            }
            self.next_age = self.items.len() as u128;
        }

        let index = self.items.len();
        self.items.push(Entry {
            item: Item {
                key: key.clone(),
                priority,
                value: value.to_owned(),
            },
            age: self.next_age,
        });
        self.next_age += 1;
        self.indices.insert(key, index);
        self.sift_up(index);
        Ok(())
    }

    pub fn peek(&self) -> Option<Item> {
        self.items.first().map(|entry| entry.item.clone())
    }

    pub fn pop(&mut self) -> Option<Item> {
        if self.items.is_empty() {
            return None;
        }

        let removed = self.remove_at(0);
        Some(removed.item)
    }

    pub fn get(&self, key: &str) -> Result<Option<Item>, Error> {
        let key = canonicalize_key(key)?;
        Ok(self
            .indices
            .get(&key)
            .map(|&index| self.items[index].item.clone()))
    }

    fn remove_at(&mut self, index: usize) -> Entry {
        let last_index = self.items.len() - 1;
        self.items.swap(index, last_index);

        let removed = self.items.pop().expect("heap index is valid");
        self.indices.remove(&removed.item.key);

        if index < self.items.len() {
            self.indices
                .insert(self.items[index].item.key.clone(), index);
            if index > 0 && self.less(index, (index - 1) / 2) {
                self.sift_up(index);
            } else {
                self.sift_down(index);
            }
        }
        removed
    }

    fn less(&self, a: usize, b: usize) -> bool {
        let left = &self.items[a];
        let right = &self.items[b];

        left.item
            .priority
            .cmp(&right.item.priority)
            .then_with(|| left.item.value.len().cmp(&right.item.value.len()))
            .then_with(|| left.age.cmp(&right.age))
            .is_lt()
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
            let left = index * 2 + 1;
            if left >= self.items.len() {
                break;
            }

            let right = left + 1;
            let best_child = if right < self.items.len() && self.less(right, left) {
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

    fn swap_entries(&mut self, a: usize, b: usize) {
        self.items.swap(a, b);
        self.indices.insert(self.items[a].item.key.clone(), a);
        self.indices.insert(self.items[b].item.key.clone(), b);
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