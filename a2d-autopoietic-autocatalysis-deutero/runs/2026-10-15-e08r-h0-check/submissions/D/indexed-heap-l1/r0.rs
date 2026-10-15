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

pub struct IndexedHeap {
    items: Vec<HeapItem>,
    indices: HashMap<String, usize>,
    next_age: u128,
}

struct HeapItem {
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

        if self.next_age == u128::MAX {
            self.renumber_ages();
        }

        let index = self.items.len();
        self.items.push(HeapItem {
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

        let last = self.items.len() - 1;
        self.swap_slots(0, last);
        let removed = self.items.pop().expect("heap was checked as nonempty");
        self.indices.remove(&removed.item.key);

        if !self.items.is_empty() {
            self.sift_down(0);
        }

        Some(removed.item)
    }

    pub fn get(&self, key: &str) -> Result<Option<Item>, Error> {
        let key = canonicalize_key(key)?;
        Ok(self
            .indices
            .get(&key)
            .map(|&index| self.items[index].item.clone()))
    }

    fn less(&self, left: usize, right: usize) -> bool {
        let a = &self.items[left];
        let b = &self.items[right];
        (a.item.priority, a.item.value.len(), a.age)
            < (b.item.priority, b.item.value.len(), b.age)
    }

    fn sift_up(&mut self, mut index: usize) {
        while index > 0 {
            let parent = (index - 1) / 2;
            if !self.less(index, parent) {
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
            let best = if right < self.items.len() && self.less(right, left) {
                right
            } else {
                left
            };
            if !self.less(best, index) {
                break;
            }
            self.swap_slots(index, best);
            index = best;
        }
    }

    fn swap_slots(&mut self, left: usize, right: usize) {
        if left == right {
            return;
        }
        self.items.swap(left, right);
        self.indices
            .insert(self.items[left].item.key.clone(), left);
        self.indices
            .insert(self.items[right].item.key.clone(), right);
    }

    fn renumber_ages(&mut self) {
        let mut order: Vec<usize> = (0..self.items.len()).collect();
        order.sort_unstable_by_key(|&index| self.items[index].age);
        for (age, index) in order.into_iter().enumerate() {
            self.items[index].age = age as u128;
        }
        self.next_age = self.items.len() as u128;
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
            byte.is_ascii_alphabetic() || byte.is_ascii_digit() || byte == b'-'
        };
        if !valid {
            return Err(Error::InvalidKey { at: start + offset });
        }
    }

    Ok(input[start..end].to_ascii_lowercase())
}