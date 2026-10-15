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
    items: Vec<HeapEntry>,
    indices: HashMap<String, usize>,
    next_age: u128,
}

struct HeapEntry {
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

        let entry = HeapEntry {
            item: Item {
                key: key.clone(),
                priority,
                value: value.to_owned(),
            },
            age: self.next_age,
        };
        self.next_age = self.next_age.wrapping_add(1);

        let index = self.items.len();
        self.items.push(entry);
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
        let removed = self.items.pop().expect("heap was nonempty");
        self.indices.remove(&removed.item.key);

        if !self.items.is_empty() {
            self.sift_down(0);
        }

        Some(removed.item)
    }

    pub fn get(&self, key: &str) -> Result<Option<Item>, Error> {
        let key = canonicalize_key(key)?;
        Ok(self.indices.get(&key).map(|&index| self.items[index].item.clone()))
    }

    fn sift_up(&mut self, mut index: usize) {
        while index > 0 {
            let parent = (index - 1) / 2;
            if !Self::better(&self.items[index], &self.items[parent]) {
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
            let best_child = if right < self.items.len()
                && Self::better(&self.items[right], &self.items[left])
            {
                right
            } else {
                left
            };

            if !Self::better(&self.items[best_child], &self.items[index]) {
                break;
            }

            self.swap_slots(index, best_child);
            index = best_child;
        }
    }

    fn swap_slots(&mut self, a: usize, b: usize) {
        self.items.swap(a, b);
        let key_a = self.items[a].item.key.clone();
        let key_b = self.items[b].item.key.clone();
        self.indices.insert(key_a, a);
        self.indices.insert(key_b, b);
    }

    fn better(a: &HeapEntry, b: &HeapEntry) -> bool {
        a.item
            .priority
            .cmp(&b.item.priority)
            .then_with(|| a.item.value.len().cmp(&b.item.value.len()))
            .then_with(|| a.age.cmp(&b.age))
            .is_lt()
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