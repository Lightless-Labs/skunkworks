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
        Self::with_optional_capacity(None)
    }

    pub fn with_capacity(capacity: usize) -> Self {
        Self::with_optional_capacity(Some(capacity))
    }

    fn with_optional_capacity(capacity: Option<usize>) -> Self {
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
                let worst_key = self.worst_key().expect("nonempty full queue");
                let worst = self.entries.get(&worst_key).expect("key exists");
                if compare_parts(
                    priority,
                    proposed.value.len(),
                    self.next_age,
                    worst.item.priority,
                    worst.item.value.len(),
                    worst.age,
                ) != std::cmp::Ordering::Less
                {
                    return Ok(Admission::Discarded(proposed));
                }
                let evicted = self.entries.remove(&worst_key).expect("key exists").item;
                self.insert_item(proposed);
                return Ok(Admission::Evicted(evicted));
            }
        }

        self.insert_item(proposed);
        Ok(Admission::Inserted)
    }

    pub fn peek(&self) -> Option<Item> {
        self.best_key()
            .and_then(|key| self.entries.get(&key))
            .map(|entry| entry.item.clone())
    }

    pub fn pop(&mut self) -> Option<Item> {
        let key = self.best_key()?;
        self.entries.remove(&key).map(|entry| entry.item)
    }

    pub fn get(&self, key: &str) -> Result<Option<Item>, Error> {
        let key = canonicalize_key(key)?;
        Ok(self.entries.get(&key).map(|entry| entry.item.clone()))
    }

    pub fn decrease_key(&mut self, key: &str, priority: i64) -> Result<i64, Error> {
        let key = canonicalize_key(key)?;
        let current = self
            .entries
            .get(&key)
            .ok_or_else(|| Error::Missing { key: key.clone() })?
            .item
            .priority;
        if priority >= current {
            return Err(Error::WrongDirection {
                current,
                requested: priority,
            });
        }
        self.entries.get_mut(&key).expect("key exists").item.priority = priority;
        Ok(current)
    }

    pub fn increase_key(&mut self, key: &str, priority: i64) -> Result<i64, Error> {
        let key = canonicalize_key(key)?;
        let current = self
            .entries
            .get(&key)
            .ok_or_else(|| Error::Missing { key: key.clone() })?
            .item
            .priority;
        if priority <= current {
            return Err(Error::WrongDirection {
                current,
                requested: priority,
            });
        }
        self.entries.get_mut(&key).expect("key exists").item.priority = priority;
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

    pub fn iter(&self) -> std::vec::IntoIter<Item> {
        let mut entries: Vec<&Entry> = self.entries.values().collect();
        entries.sort_by(|a, b| compare_entries(a, b));
        entries
            .into_iter()
            .map(|entry| entry.item.clone())
            .collect::<Vec<_>>()
            .into_iter()
    }

    pub fn set_capacity(&mut self, capacity: Option<usize>) -> Vec<Item> {
        self.capacity = capacity;
        let mut evicted = Vec::new();
        if let Some(limit) = capacity {
            while self.entries.len() > limit {
                let key = self.worst_key().expect("queue is nonempty");
                evicted.push(self.entries.remove(&key).expect("key exists").item);
            }
        }
        evicted
    }

    pub fn merge(
        &mut self,
        other: &mut Self,
        policy: CollisionPolicy,
    ) -> Result<MergeReport, Error> {
        let mut source: Vec<Entry> = other.entries.values().cloned().collect();
        source.sort_by(compare_entries);

        if policy == CollisionPolicy::Reject {
            for entry in &source {
                if self.entries.contains_key(&entry.item.key) {
                    return Err(Error::Collision {
                        key: entry.item.key.clone(),
                    });
                }
            }
        }

        let mut report = MergeReport {
            moved: 0,
            replaced: 0,
            discarded: 0,
            evicted: Vec::new(),
        };

        for entry in source {
            if let Some(receiver) = self.entries.get_mut(&entry.item.key) {
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
                report.moved += 1;
                let item = entry.item;
                self.insert_item(item);
            }
        }

        other.entries.clear();

        if let Some(limit) = self.capacity {
            while self.entries.len() > limit {
                let key = self.worst_key().expect("queue is nonempty");
                report
                    .evicted
                    .push(self.entries.remove(&key).expect("key exists").item);
            }
        }

        Ok(report)
    }

    fn insert_item(&mut self, item: Item) {
        let age = self.next_age;
        self.next_age = self.next_age.wrapping_add(1);
        self.entries.insert(item.key.clone(), Entry { item, age });
    }

    fn best_key(&self) -> Option<String> {
        self.entries
            .values()
            .min_by(|a, b| compare_entries(a, b))
            .map(|entry| entry.item.key.clone())
    }

    fn worst_key(&self) -> Option<String> {
        self.entries
            .values()
            .max_by(|a, b| compare_entries(a, b))
            .map(|entry| entry.item.key.clone())
    }
}

impl Default for IndexedHeap {
    fn default() -> Self {
        Self::new()
    }
}

fn compare_entries(a: &Entry, b: &Entry) -> std::cmp::Ordering {
    compare_parts(
        a.item.priority,
        a.item.value.len(),
        a.age,
        b.item.priority,
        b.item.value.len(),
        b.age,
    )
}

fn compare_parts(
    priority_a: i64,
    value_len_a: usize,
    age_a: u128,
    priority_b: i64,
    value_len_b: usize,
    age_b: u128,
) -> std::cmp::Ordering {
    priority_a
        .cmp(&priority_b)
        .then_with(|| value_len_a.cmp(&value_len_b))
        .then_with(|| age_a.cmp(&age_b))
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