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
}

#[derive(Clone)]
struct Entry {
    item: Item,
    age: u128,
}

pub struct IndexedHeap {
    entries: HashMap<String, Entry>,
    next_age: u128,
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
    for (offset, &b) in bytes[start..end].iter().enumerate() {
        let valid = if offset == 0 {
            b.is_ascii_alphabetic()
        } else {
            b.is_ascii_alphabetic() || b.is_ascii_digit() || b == b'-'
        };
        if !valid {
            return Err(Error::InvalidKey { at: start + offset });
        }
    }
    Ok(input[start..end].to_ascii_lowercase())
}

impl IndexedHeap {
    pub fn new() -> Self {
        Self {
            entries: HashMap::new(),
            next_age: 0,
        }
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    fn rank_cmp(a: &Entry, b: &Entry) -> std::cmp::Ordering {
        a.item
            .priority
            .cmp(&b.item.priority)
            .then_with(|| a.item.value.len().cmp(&b.item.value.len()))
            .then_with(|| a.age.cmp(&b.age))
    }

    fn best_key(&self) -> Option<&str> {
        self.entries
            .values()
            .min_by(|a, b| Self::rank_cmp(a, b))
            .map(|e| e.item.key.as_str())
    }

    fn renumber_ages(&mut self) {
        let mut keys: Vec<String> = self.entries.keys().cloned().collect();
        keys.sort_by_key(|k| self.entries.get(k).unwrap().age);
        for (age, key) in keys.into_iter().enumerate() {
            self.entries.get_mut(&key).unwrap().age = age as u128;
        }
        self.next_age = self.entries.len() as u128;
    }

    pub fn push(&mut self, key: &str, priority: i64, value: &str) -> Result<(), Error> {
        let key = canonical_key(key)?;
        if self.entries.contains_key(&key) {
            return Err(Error::Duplicate { key });
        }
        if self.next_age == u128::MAX {
            self.renumber_ages();
        }
        self.entries.insert(
            key.clone(),
            Entry {
                item: Item {
                    key,
                    priority,
                    value: value.to_owned(),
                },
                age: self.next_age,
            },
        );
        self.next_age += 1;
        Ok(())
    }

    pub fn peek(&self) -> Option<Item> {
        self.best_key()
            .and_then(|key| self.entries.get(key))
            .map(|entry| entry.item.clone())
    }

    pub fn pop(&mut self) -> Option<Item> {
        let key = self.best_key()?.to_owned();
        self.entries.remove(&key).map(|entry| entry.item)
    }

    pub fn get(&self, key: &str) -> Result<Option<Item>, Error> {
        let key = canonical_key(key)?;
        Ok(self.entries.get(&key).map(|entry| entry.item.clone()))
    }

    pub fn decrease_key(&mut self, key: &str, priority: i64) -> Result<i64, Error> {
        let key = canonical_key(key)?;
        let entry = self.entries.get_mut(&key).ok_or_else(|| Error::Missing { key })?;
        let current = entry.item.priority;
        if priority >= current {
            return Err(Error::WrongDirection { current, requested: priority });
        }
        entry.item.priority = priority;
        Ok(current)
    }

    pub fn increase_key(&mut self, key: &str, priority: i64) -> Result<i64, Error> {
        let key = canonical_key(key)?;
        let entry = self.entries.get_mut(&key).ok_or_else(|| Error::Missing { key })?;
        let current = entry.item.priority;
        if priority <= current {
            return Err(Error::WrongDirection { current, requested: priority });
        }
        entry.item.priority = priority;
        Ok(current)
    }

    pub fn replace_value(&mut self, key: &str, value: &str) -> Result<String, Error> {
        let key = canonical_key(key)?;
        let entry = self.entries.get_mut(&key).ok_or_else(|| Error::Missing { key })?;
        Ok(std::mem::replace(&mut entry.item.value, value.to_owned()))
    }

    pub fn remove(&mut self, key: &str) -> Result<Item, Error> {
        let key = canonical_key(key)?;
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
        let mut source: Vec<Entry> = other.entries.values().cloned().collect();
        source.sort_by(Self::rank_cmp);

        if policy == CollisionPolicy::Reject {
            for entry in &source {
                if self.entries.contains_key(&entry.item.key) {
                    return Err(Error::Collision {
                        key: entry.item.key.clone(),
                    });
                }
            }
        }

        let mut report = MergeReport { moved: 0, replaced: 0, discarded: 0 };
        for entry in source {
            let key = entry.item.key.clone();
            if let Some(receiver) = self.entries.get_mut(&key) {
                match policy {
                    CollisionPolicy::KeepSelf => report.discarded += 1,
                    CollisionPolicy::TakeOther => {
                        receiver.item.priority = entry.item.priority;
                        receiver.item.value = entry.item.value;
                        report.replaced += 1;
                    }
                    CollisionPolicy::Reject => unreachable!(),
                }
            } else {
                if self.next_age == u128::MAX {
                    self.renumber_ages();
                }
                self.entries.insert(
                    key,
                    Entry {
                        item: entry.item,
                        age: self.next_age,
                    },
                );
                self.next_age += 1;
                report.moved += 1;
            }
        }
        other.entries.clear();
        other.next_age = 0;
        Ok(report)
    }
}

impl Default for IndexedHeap {
    fn default() -> Self {
        Self::new()
    }
}
