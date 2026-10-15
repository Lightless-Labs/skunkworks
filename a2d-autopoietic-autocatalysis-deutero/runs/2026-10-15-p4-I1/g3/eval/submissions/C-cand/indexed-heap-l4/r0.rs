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
    entries: Vec<Entry>,
    indices: HashMap<String, usize>,
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
            entries: Vec::new(),
            indices: HashMap::new(),
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
        let key = canonical_key(key)?;
        if self.indices.contains_key(&key) {
            return Err(Error::Duplicate { key });
        }

        let proposed = Entry {
            item: Item {
                key,
                priority,
                value: value.to_owned(),
            },
            age: self.next_age,
        };

        if self.capacity == Some(0) {
            return Ok(Admission::Discarded(proposed.item));
        }

        if let Some(capacity) = self.capacity {
            if self.entries.len() >= capacity {
                let worst = self.worst_index().expect("nonempty full queue");
                if !entry_better(&proposed, &self.entries[worst]) {
                    return Ok(Admission::Discarded(proposed.item));
                }
                let evicted = self.remove_index(worst).item;
                self.insert_entry(proposed);
                self.advance_age();
                return Ok(Admission::Evicted(evicted));
            }
        }

        self.insert_entry(proposed);
        self.advance_age();
        Ok(Admission::Inserted)
    }

    pub fn peek(&self) -> Option<Item> {
        self.best_index()
            .map(|index| self.entries[index].item.clone())
    }

    pub fn pop(&mut self) -> Option<Item> {
        self.best_index().map(|index| self.remove_index(index).item)
    }

    pub fn get(&self, key: &str) -> Result<Option<Item>, Error> {
        let key = canonical_key(key)?;
        Ok(self
            .indices
            .get(&key)
            .map(|&index| self.entries[index].item.clone()))
    }

    pub fn decrease_key(&mut self, key: &str, priority: i64) -> Result<i64, Error> {
        let key = canonical_key(key)?;
        let index = *self
            .indices
            .get(&key)
            .ok_or_else(|| Error::Missing { key: key.clone() })?;
        let current = self.entries[index].item.priority;
        if priority >= current {
            return Err(Error::WrongDirection {
                current,
                requested: priority,
            });
        }
        self.entries[index].item.priority = priority;
        Ok(current)
    }

    pub fn increase_key(&mut self, key: &str, priority: i64) -> Result<i64, Error> {
        let key = canonical_key(key)?;
        let index = *self
            .indices
            .get(&key)
            .ok_or_else(|| Error::Missing { key: key.clone() })?;
        let current = self.entries[index].item.priority;
        if priority <= current {
            return Err(Error::WrongDirection {
                current,
                requested: priority,
            });
        }
        self.entries[index].item.priority = priority;
        Ok(current)
    }

    pub fn replace_value(&mut self, key: &str, value: &str) -> Result<String, Error> {
        let key = canonical_key(key)?;
        let index = *self
            .indices
            .get(&key)
            .ok_or_else(|| Error::Missing { key: key.clone() })?;
        Ok(std::mem::replace(
            &mut self.entries[index].item.value,
            value.to_owned(),
        ))
    }

    pub fn remove(&mut self, key: &str) -> Result<Item, Error> {
        let key = canonical_key(key)?;
        let index = *self
            .indices
            .get(&key)
            .ok_or_else(|| Error::Missing { key: key.clone() })?;
        Ok(self.remove_index(index).item)
    }

    pub fn set_capacity(&mut self, capacity: Option<usize>) -> Vec<Item> {
        self.capacity = capacity;
        let mut evicted = Vec::new();
        while self
            .capacity
            .is_some_and(|limit| self.entries.len() > limit)
        {
            let index = self.worst_index().expect("excess entries exist");
            evicted.push(self.remove_index(index).item);
        }
        evicted
    }

    pub fn iter(&self) -> std::vec::IntoIter<Item> {
        let mut indices: Vec<usize> = (0..self.entries.len()).collect();
        indices.sort_by(|&a, &b| entry_order(&self.entries[a], &self.entries[b]));
        indices
            .into_iter()
            .map(|index| self.entries[index].item.clone())
            .collect::<Vec<_>>()
            .into_iter()
    }

    pub fn merge(
        &mut self,
        other: &mut Self,
        policy: CollisionPolicy,
    ) -> Result<MergeReport, Error> {
        let mut source_indices: Vec<usize> = (0..other.entries.len()).collect();
        source_indices.sort_by(|&a, &b| entry_order(&other.entries[a], &other.entries[b]));

        if policy == CollisionPolicy::Reject {
            for &index in &source_indices {
                let key = &other.entries[index].item.key;
                if self.indices.contains_key(key) {
                    return Err(Error::Collision { key: key.clone() });
                }
            }
        }

        let mut report = MergeReport {
            moved: 0,
            replaced: 0,
            discarded: 0,
            evicted: Vec::new(),
        };

        let source: Vec<Entry> = source_indices
            .into_iter()
            .map(|index| other.entries[index].clone())
            .collect();

        for mut entry in source {
            if let Some(&index) = self.indices.get(&entry.item.key) {
                match policy {
                    CollisionPolicy::KeepSelf => report.discarded += 1,
                    CollisionPolicy::TakeOther => {
                        self.entries[index].item.priority = entry.item.priority;
                        self.entries[index].item.value = std::mem::take(&mut entry.item.value);
                        report.replaced += 1;
                    }
                    CollisionPolicy::Reject => unreachable!(),
                }
            } else {
                entry.age = self.next_age;
                self.insert_entry(entry);
                self.advance_age();
                report.moved += 1;
            }
        }

        other.entries.clear();
        other.indices.clear();

        while self
            .capacity
            .is_some_and(|limit| self.entries.len() > limit)
        {
            let index = self.worst_index().expect("excess entries exist");
            report.evicted.push(self.remove_index(index).item);
        }

        Ok(report)
    }

    fn advance_age(&mut self) {
        // Renumber in age order before the counter could overflow.
        if self.next_age == u128::MAX {
            let mut order: Vec<usize> = (0..self.entries.len()).collect();
            order.sort_by_key(|&i| self.entries[i].age);
            for (age, index) in order.into_iter().enumerate() {
                self.entries[index].age = age as u128;
            }
            self.next_age = self.entries.len() as u128;
        } else {
            self.next_age += 1;
        }
    }

    fn insert_entry(&mut self, entry: Entry) {
        let index = self.entries.len();
        self.indices.insert(entry.item.key.clone(), index);
        self.entries.push(entry);
    }

    fn remove_index(&mut self, index: usize) -> Entry {
        let removed = self.entries.swap_remove(index);
        self.indices.remove(&removed.item.key);
        if index < self.entries.len() {
            self.indices
                .insert(self.entries[index].item.key.clone(), index);
        }
        removed
    }

    fn best_index(&self) -> Option<usize> {
        self.entries
            .iter()
            .enumerate()
            .min_by(|(_, a), (_, b)| entry_order(a, b))
            .map(|(index, _)| index)
    }

    fn worst_index(&self) -> Option<usize> {
        self.entries
            .iter()
            .enumerate()
            .max_by(|(_, a), (_, b)| entry_order(a, b))
            .map(|(index, _)| index)
    }
}

fn entry_order(a: &Entry, b: &Entry) -> std::cmp::Ordering {
    a.item
        .priority
        .cmp(&b.item.priority)
        .then_with(|| a.item.value.len().cmp(&b.item.value.len()))
        .then_with(|| a.age.cmp(&b.age))
}

fn entry_better(a: &Entry, b: &Entry) -> bool {
    entry_order(a, b).is_lt()
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
            byte.is_ascii_alphanumeric() || byte == b'-'
        };
        if !valid {
            return Err(Error::InvalidKey { at: start + relative });
        }
    }

    let mut canonical = input[start..end].to_owned();
    canonical.make_ascii_lowercase();
    Ok(canonical)
}

fn main() {}