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
        self.items.is_empty()
    }

    pub fn push(
        &mut self,
        key: &str,
        priority: i64,
        value: &str,
    ) -> Result<(), Error> {
        let canonical_key = canonicalize_key(key)?;
        if self.indices.contains_key(&canonical_key) {
            return Err(Error::Duplicate { key: canonical_key });
        }

        // Rebase ages before the counter could wrap, preserving their order.
        if self.next_age == u128::MAX {
            let mut order: Vec<usize> = (0..self.items.len()).collect();
            order.sort_by_key(|&i| self.items[i].age);
            for (age, &index) in order.iter().enumerate() {
                self.items[index].age = age as u128;
            }
            self.next_age = self.items.len() as u128;
        }

        let entry = Entry {
            item: Item {
                key: canonical_key.clone(),
                priority,
                value: value.to_owned(),
            },
            age: self.next_age,
        };
        self.next_age += 1;
        let index = self.items.len();
        self.items.push(entry);
        self.indices.insert(canonical_key, index);
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

        let last = self.items.len() - 1;
        self.swap_slots(0, last);
        let removed = self.items.pop().expect("heap was non-empty");
        self.indices.remove(&removed.item.key);

        if !self.items.is_empty() {
            self.sift_down(0);
        }

        Some(removed.item)
    }

    pub fn get(&self, key: &str) -> Result<Option<Item>, Error> {
        let canonical_key = canonicalize_key(key)?;
        Ok(self
            .indices
            .get(&canonical_key)
            .map(|&index| self.items[index].item.clone()))
    }

    fn is_better(&self, left: usize, right: usize) -> bool {
        let a = &self.items[left];
        let b = &self.items[right];
        (a.item.priority, a.item.value.len(), a.age)
            < (b.item.priority, b.item.value.len(), b.age)
    }

    fn swap_slots(&mut self, a: usize, b: usize) {
        self.items.swap(a, b);
        let key_a = self.items[a].item.key.clone();
        let key_b = self.items[b].item.key.clone();
        self.indices.insert(key_a, a);
        self.indices.insert(key_b, b);
    }

    fn sift_up(&mut self, mut index: usize) {
        while index > 0 {
            let parent = (index - 1) / 2;
            if !self.is_better(index, parent) {
                break;
            }
            self.swap_slots(index, parent);
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
            let best_child = if right < self.items.len() && self.is_better(right, left) {
                right
            } else {
                left
            };

            if !self.is_better(best_child, index) {
                break;
            }
            self.swap_slots(index, best_child);
            index = best_child;
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

    for (relative, &byte) in bytes[start..end].iter().enumerate() {
        let valid = if relative == 0 {
            byte.is_ascii_alphabetic()
        } else {
            byte.is_ascii_alphabetic() || byte.is_ascii_digit() || byte == b'-'
        };
        if !valid {
            return Err(Error::InvalidKey { at: start + relative });
        }
    }

    Ok(input[start..end].to_ascii_lowercase())
}