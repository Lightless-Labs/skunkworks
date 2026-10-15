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
    items: Vec<(Item, u128)>,
    indices: HashMap<String, usize>,
    next_age: u128,
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

    pub fn push(&mut self, key: &str, priority: i64, value: &str) -> Result<(), Error> {
        let key = canonicalize_key(key)?;
        if self.indices.contains_key(&key) {
            return Err(Error::Duplicate { key });
        }

        let age = self.next_age;
        self.next_age = self.next_age.wrapping_add(1);
        let index = self.items.len();
        self.items.push((
            Item {
                key: key.clone(),
                priority,
                value: value.to_owned(),
            },
            age,
        ));
        self.indices.insert(key, index);
        self.sift_up(index);
        Ok(())
    }

    pub fn peek(&self) -> Option<Item> {
        self.items.first().map(|(item, _)| item.clone())
    }

    pub fn pop(&mut self) -> Option<Item> {
        if self.items.is_empty() {
            return None;
        }

        let last = self.items.len() - 1;
        self.items.swap(0, last);
        let (item, _) = self.items.pop().unwrap();
        self.indices.remove(&item.key);

        if !self.items.is_empty() {
            self.update_index(0);
            self.sift_down(0);
        }

        Some(item)
    }

    pub fn get(&self, key: &str) -> Result<Option<Item>, Error> {
        let key = canonicalize_key(key)?;
        Ok(self
            .indices
            .get(&key)
            .map(|&index| self.items[index].0.clone()))
    }

    fn is_better(&self, left: usize, right: usize) -> bool {
        let (left_item, left_age) = &self.items[left];
        let (right_item, right_age) = &self.items[right];
        left_item
            .priority
            .cmp(&right_item.priority)
            .then_with(|| left_item.value.len().cmp(&right_item.value.len()))
            .then_with(|| left_age.cmp(right_age))
            .is_lt()
    }

    fn update_index(&mut self, index: usize) {
        self.indices
            .insert(self.items[index].0.key.clone(), index);
    }

    fn swap_slots(&mut self, left: usize, right: usize) {
        self.items.swap(left, right);
        self.update_index(left);
        self.update_index(right);
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
            let best = if right < self.items.len() && self.is_better(right, left) {
                right
            } else {
                left
            };

            if !self.is_better(best, index) {
                break;
            }
            self.swap_slots(index, best);
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