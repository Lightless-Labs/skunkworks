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
        let key = canonical_key(key)?;
        if self.items.iter().any(|entry| entry.item.key == key) {
            return Err(Error::Duplicate { key });
        }

        // Renumber retained entries if the age counter is exhausted. This preserves
        // their relative insertion order before assigning the new, youngest age.
        if self.next_age == u128::MAX {
            let mut order: Vec<usize> = (0..self.items.len()).collect();
            order.sort_by_key(|&i| self.items[i].age);
            for (age, i) in order.into_iter().enumerate() {
                self.items[i].age = age as u128;
            }
            self.next_age = self.items.len() as u128;
        }

        self.items.push(Entry {
            item: Item {
                key,
                priority,
                value: value.to_owned(),
            },
            age: self.next_age,
        });
        self.next_age += 1;
        Ok(())
    }

    pub fn peek(&self) -> Option<Item> {
        self.items
            .iter()
            .min_by(|a, b| compare_entries(a, b))
            .map(|entry| entry.item.clone())
    }

    pub fn pop(&mut self) -> Option<Item> {
        let best = self
            .items
            .iter()
            .enumerate()
            .min_by(|(_, a), (_, b)| compare_entries(a, b))
            .map(|(index, _)| index)?;
        Some(self.items.swap_remove(best).item)
    }

    pub fn get(&self, key: &str) -> Result<Option<Item>, Error> {
        let key = canonical_key(key)?;
        Ok(self
            .items
            .iter()
            .find(|entry| entry.item.key == key)
            .map(|entry| entry.item.clone()))
    }
}

fn compare_entries(a: &Entry, b: &Entry) -> std::cmp::Ordering {
    a.item
        .priority
        .cmp(&b.item.priority)
        .then_with(|| a.item.value.len().cmp(&b.item.value.len()))
        .then_with(|| a.age.cmp(&b.age))
}

fn canonical_key(input: &str) -> Result<String, Error> {
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

fn main() {}