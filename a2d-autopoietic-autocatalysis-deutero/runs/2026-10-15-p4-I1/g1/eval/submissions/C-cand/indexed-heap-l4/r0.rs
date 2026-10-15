use std::cmp::Ordering;
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
    Missing { key: String },
    WrongDirection { current: i64, requested: i64 },
    Collision { key: String },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CollisionPolicy {
    KeepSelf,
    TakeOther,
    Reject,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MergeReport {
    pub moved: usize,
    pub replaced: usize,
    pub discarded: usize,
    pub evicted: Vec<Item>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Admission {
    Inserted,
    Evicted(Item),
    Discarded(Item),
}

#[derive(Clone)]
struct Entry {
    item: Item,
    age: u128,
}

pub struct IndexedHeap {
    entries: HashMap<String, Entry>,
    capacity: Option<usize>,
    next_age: u128,
}

impl IndexedHeap {
    pub fn new() -> Self {
        Self::with_capacity_option(None)
    }

    pub fn with_capacity(capacity: usize) -> Self {
        Self::with_capacity_option(Some(capacity))
    }

    fn with_capacity_option(capacity: Option<usize>) -> Self {
        Self {
            entries: HashMap::new(),
            capacity,
            next_age: 0,
        }
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    pub fn capacity(&self) -> Option<usize> {
        self.capacity
    }

    pub fn push(
        &mut self,
        key: &str,
        priority: i64,
        value: &str,
    ) -> Result<Admission, Error> {
        let key = canonicalize_key(key)?;
        if self.entries.contains_key(&key) {
            return Err(Error::Duplicate { key });
        }

        let proposed = Item {
            key: key.clone(),
            priority,
            value: value.to_owned(),
        };

        if self.capacity == Some(0) {
            return Ok(Admission::Discarded(proposed));
        }

        if let Some(limit) = self.capacity {
            if self.entries.len() >= limit {
                if let Some((worst_key, worst)) = self.worst_entry() {
                    let proposed_entry = Entry {
                        item: proposed.clone(),
                        age: self.next_age,
                    };
                    if compare_entries(&proposed_entry, worst) != Ordering::Less {
                        return Ok(Admission::Discarded(proposed));
                    }
                    let evicted = self.entries.remove(&worst_key).unwrap().item;
                    self.insert_new(proposed);
                    return Ok(Admission::Evicted(evicted));
                }
            }
        }

        self.insert_new(proposed);
        Ok(Admission::Inserted)
    }

    pub fn peek(&self) -> Option<Item> {
        self.best_entry().map(|(_, entry)| entry.item.clone())
    }

    pub fn pop(&mut self) -> Option<Item> {
        let key = self.best_entry()?.0;
        self.entries.remove(&key).map(|entry| entry.item)
    }

    pub fn get(&self, key: &str) -> Result<Option<Item>, Error> {
        let key = canonicalize_key(key)?;
        Ok(self.entries.get(&key).map(|entry| entry.item.clone()))
    }

    pub fn decrease_key(&mut self, key: &str, priority: i64) -> Result<i64, Error> {
        let key = canonicalize_key(key)?;
        let entry = self
            .entries
            .get_mut(&key)
            .ok_or_else(|| Error::Missing { key: key.clone() })?;
        let current = entry.item.priority;
        if priority >= current {
            return Err(Error::WrongDirection {
                current,
                requested: priority,
            });
        }
        entry.item.priority = priority;
        Ok(current)
    }

    pub fn increase_key(&mut self, key: &str, priority: i64) -> Result<i64, Error> {
        let key = canonicalize_key(key)?;
        let entry = self
            .entries
            .get_mut(&key)
            .ok_or_else(|| Error::Missing { key: key.clone() })?;
        let current = entry.item.priority;
        if priority <= current {
            return Err(Error::WrongDirection {
                current,
                requested: priority,
            });
        }
        entry.item.priority = priority;
        Ok(current)
    }

    pub fn replace_value(&mut self, key: &str, value: &str) -> Result<String, Error> {
        let key = canonicalize_key(key)?;
        let entry = self
            .entries
            .get_mut(&key)
            .ok_or_else(|| Error::Missing { key: key.clone() })?;
        Ok(std::mem::replace(&mut entry.item.value, value.to_owned()))
    }

    pub fn remove(&mut self, key: &str) -> Result<Item, Error> {
        let key = canonicalize_key(key)?;
        self.entries
            .remove(&key)
            .map(|entry| entry.item)
            .ok_or(Error::Missing { key })
    }

    pub fn merge(
        &mut self,
        other: &mut Self,
        policy: CollisionPolicy,
    ) -> Result<MergeReport, Error> {
        let source_items = other.ordered_entries();
        if policy == CollisionPolicy::Reject {
            for entry in &source_items {
                if self.entries.contains_key(&entry.item.key) {
                    return Err(Error::Collision {
                        key: entry.item.key.clone(),
                    });
                }
            }
        }

        let mut moved = 0;
        let mut replaced = 0;
        let mut discarded = 0;

        for source in source_items {
            match self.entries.get_mut(&source.item.key) {
                Some(receiver) => match policy {
                    CollisionPolicy::KeepSelf => discarded += 1,
                    CollisionPolicy::TakeOther => {
                        receiver.item.priority = source.item.priority;
                        receiver.item.value = source.item.value;
                        replaced += 1;
                    }
                    CollisionPolicy::Reject => unreachable!(),
                },
                None => {
                    self.insert_new(source.item);
                    moved += 1;
                }
            }
        }

        other.entries.clear();

        let evicted = self.trim_to_capacity();
        Ok(MergeReport {
            moved,
            replaced,
            discarded,
            evicted,
        })
    }

    pub fn set_capacity(&mut self, capacity: Option<usize>) -> Vec<Item> {
        self.capacity = capacity;
        self.trim_to_capacity()
    }

    pub fn iter(&self) -> std::vec::IntoIter<Item> {
        self.ordered_entries()
            .into_iter()
            .map(|entry| entry.item)
            .collect::<Vec<_>>()
            .into_iter()
    }

    fn insert_new(&mut self, item: Item) {
        let age = self.next_age;
        self.next_age = self.next_age.wrapping_add(1);
        self.entries.insert(item.key.clone(), Entry { item, age });
    }

    fn best_entry(&self) -> Option<(String, &Entry)> {
        self.entries
            .iter()
            .min_by(|(_, a), (_, b)| compare_entries(a, b))
            .map(|(key, entry)| (key.clone(), entry))
    }

    fn worst_entry(&self) -> Option<(String, &Entry)> {
        self.entries
            .iter()
            .max_by(|(_, a), (_, b)| compare_entries(a, b))
            .map(|(key, entry)| (key.clone(), entry))
    }

    fn ordered_entries(&self) -> Vec<Entry> {
        let mut entries: Vec<Entry> = self.entries.values().cloned().collect();
        entries.sort_by(compare_entries);
        entries
    }

    fn trim_to_capacity(&mut self) -> Vec<Item> {
        let Some(limit) = self.capacity else {
            return Vec::new();
        };
        let excess = self.entries.len().saturating_sub(limit);
        let mut evicted = Vec::with_capacity(excess);
        for _ in 0..excess {
            if let Some((key, _)) = self.worst_entry() {
                evicted.push(self.entries.remove(&key).unwrap().item);
            }
        }
        evicted
    }
}

fn compare_entries(a: &Entry, b: &Entry) -> Ordering {
    a.item
        .priority
        .cmp(&b.item.priority)
        .then_with(|| a.item.value.len().cmp(&b.item.value.len()))
        .then_with(|| a.age.cmp(&b.age))
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