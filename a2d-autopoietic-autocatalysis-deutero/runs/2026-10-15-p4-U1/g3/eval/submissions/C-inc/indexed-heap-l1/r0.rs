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

#[derive(Debug, Clone)]
struct Entry {
    item: Item,
    age: u128,
}

pub struct IndexedHeap {
    entries: Vec<Entry>,
    next_age: u128,
}

impl IndexedHeap {
    pub fn new() -> Self {
        Self {
            entries: Vec::new(),
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
        if self.entries.iter().any(|entry| entry.item.key == key) {
            return Err(Error::Duplicate { key });
        }

        let age = self.next_age;
        self.next_age = self.next_age.wrapping_add(1);
        self.entries.push(Entry {
            item: Item {
                key,
                priority,
                value: value.to_owned(),
            },
            age,
        });
        Ok(())
    }

    pub fn peek(&self) -> Option<Item> {
        self.entries
            .iter()
            .min_by(|a, b| compare_entries(a, b))
            .map(|entry| entry.item.clone())
    }

    pub fn pop(&mut self) -> Option<Item> {
        let best = self
            .entries
            .iter()
            .enumerate()
            .min_by(|(_, a), (_, b)| compare_entries(a, b))
            .map(|(index, _)| index)?;
        Some(self.entries.swap_remove(best).item)
    }

    pub fn get(&self, key: &str) -> Result<Option<Item>, Error> {
        let key = canonicalize_key(key)?;
        Ok(self
            .entries
            .iter()
            .find(|entry| entry.item.key == key)
            .map(|entry| entry.item.clone()))
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

fn compare_entries(a: &Entry, b: &Entry) -> std::cmp::Ordering {
    a.item
        .priority
        .cmp(&b.item.priority)
        .then_with(|| a.item.value.len().cmp(&b.item.value.len()))
        .then_with(|| a.age.cmp(&b.age))
}

fn main() {}