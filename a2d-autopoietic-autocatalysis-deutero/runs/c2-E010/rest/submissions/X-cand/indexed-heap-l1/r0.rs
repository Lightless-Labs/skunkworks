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
    items: HashMap<String, Entry>,
    next_age: u128,
}

impl IndexedHeap {
    pub fn new() -> Self {
        Self {
            items: HashMap::new(),
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
        let key = canonicalize(key)?;
        if self.items.contains_key(&key) {
            return Err(Error::Duplicate { key });
        }

        if self.next_age == u128::MAX {
            let mut keys: Vec<String> = self.items.keys().cloned().collect();
            keys.sort_by_key(|key| self.items[key].age);
            for (age, key) in keys.into_iter().enumerate() {
                self.items.get_mut(&key).unwrap().age = age as u128;
            }
            self.next_age = self.items.len() as u128;
        }

        let age = self.next_age;
        self.next_age += 1;
        self.items.insert(
            key.clone(),
            Entry {
                item: Item {
                    key,
                    priority,
                    value: value.to_owned(),
                },
                age,
            },
        );
        Ok(())
    }

    pub fn peek(&self) -> Option<Item> {
        self.best_key()
            .map(|key| self.items[key].item.clone())
    }

    pub fn pop(&mut self) -> Option<Item> {
        let key = self.best_key()?.to_owned();
        self.items.remove(&key).map(|entry| entry.item)
    }

    pub fn get(&self, key: &str) -> Result<Option<Item>, Error> {
        let key = canonicalize(key)?;
        Ok(self.items.get(&key).map(|entry| entry.item.clone()))
    }

    fn best_key(&self) -> Option<&str> {
        self.items
            .values()
            .min_by(|a, b| {
                a.item
                    .priority
                    .cmp(&b.item.priority)
                    .then_with(|| a.item.value.len().cmp(&b.item.value.len()))
                    .then_with(|| a.age.cmp(&b.age))
            })
            .map(|entry| entry.item.key.as_str())
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

    Ok(bytes[start..end]
        .iter()
        .map(|byte| byte.to_ascii_lowercase() as char)
        .collect())
}
