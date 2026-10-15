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

#[derive(Debug, Clone)]
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

fn compare_entries(a: &Entry, b: &Entry) -> Ordering {
    a.item
        .priority
        .cmp(&b.item.priority)
        .then_with(|| a.item.value.len().cmp(&b.item.value.len()))
        .then_with(|| a.age.cmp(&b.age))
}

impl IndexedHeap {
    pub fn new() -> Self {
        Self::with_capacity_internal(None)
    }

    pub fn with_capacity(capacity: usize) -> Self {
        Self::with_capacity_internal(Some(capacity))
    }

    fn with_capacity_internal(capacity: Option<usize>) -> Self {
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

    fn fresh_age(&mut self) -> u128 {
        if self.next_age == u128::MAX {
            let mut order: Vec<(usize, u128)> = self
                .entries
                .iter()
                .enumerate()
                .map(|(i, e)| (i, e.age))
                .collect();
            order.sort_by_key(|&(_, age)| age);
            for (age, (index, _)) in order.into_iter().enumerate() {
                self.entries[index].age = age as u128;
            }
            self.next_age = self.entries.len() as u128;
        }
        let age = self.next_age;
        self.next_age += 1;
        age
    }

    fn rebuild_indices(&mut self) {
        self.indices.clear();
        self.indices.reserve(self.entries.len());
        for (i, entry) in self.entries.iter().enumerate() {
            self.indices.insert(entry.item.key.clone(), i);
        }
    }

    fn best_index(&self) -> Option<usize> {
        self.entries
            .iter()
            .enumerate()
            .min_by(|(_, a), (_, b)| compare_entries(a, b))
            .map(|(i, _)| i)
    }

    fn worst_index(&self) -> Option<usize> {
        self.entries
            .iter()
            .enumerate()
            .max_by(|(_, a), (_, b)| compare_entries(a, b))
            .map(|(i, _)| i)
    }

    fn remove_index(&mut self, index: usize) -> Item {
        let entry = self.entries.remove(index);
        self.rebuild_indices();
        entry.item
    }

    fn ordered_entries(&self) -> Vec<&Entry> {
        let mut ordered: Vec<&Entry> = self.entries.iter().collect();
        ordered.sort_by(|a, b| compare_entries(a, b));
        ordered
    }

    fn trim_to_capacity(&mut self) -> Vec<Item> {
        let mut evicted = Vec::new();
        if let Some(capacity) = self.capacity {
            while self.entries.len() > capacity {
                let index = self.worst_index().expect("excess entries imply a worst entry");
                evicted.push(self.remove_index(index));
            }
        }
        evicted
    }

    pub fn push(
        &mut self,
        key: &str,
        priority: i64,
        value: &str,
    ) -> Result<Admission, Error> {
        let key = canonicalize_key(key)?;
        if self.indices.contains_key(&key) {
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

        if let Some(capacity) = self.capacity {
            if self.entries.len() >= capacity {
                let worst_index = self.worst_index().expect("nonzero full capacity is nonempty");
                let proposed_entry = Entry {
                    item: proposed.clone(),
                    age: self.next_age,
                };
                if compare_entries(&proposed_entry, &self.entries[worst_index]) != Ordering::Less {
                    return Ok(Admission::Discarded(proposed));
                }
                let evicted = self.remove_index(worst_index);
                let age = self.fresh_age();
                self.entries.push(Entry {
                    item: proposed,
                    age,
                });
                self.rebuild_indices();
                return Ok(Admission::Evicted(evicted));
            }
        }

        let age = self.fresh_age();
        self.entries.push(Entry {
            item: proposed,
            age,
        });
        self.rebuild_indices();
        Ok(Admission::Inserted)
    }

    pub fn peek(&self) -> Option<Item> {
        self.best_index().map(|i| self.entries[i].item.clone())
    }

    pub fn pop(&mut self) -> Option<Item> {
        let index = self.best_index()?;
        Some(self.remove_index(index))
    }

    pub fn get(&self, key: &str) -> Result<Option<Item>, Error> {
        let key = canonicalize_key(key)?;
        Ok(self
            .indices
            .get(&key)
            .map(|&index| self.entries[index].item.clone()))
    }

    pub fn decrease_key(&mut self, key: &str, priority: i64) -> Result<i64, Error> {
        let key = canonicalize_key(key)?;
        let Some(&index) = self.indices.get(&key) else {
            return Err(Error::Missing { key });
        };
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
        let key = canonicalize_key(key)?;
        let Some(&index) = self.indices.get(&key) else {
            return Err(Error::Missing { key });
        };
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
        let key = canonicalize_key(key)?;
        let Some(&index) = self.indices.get(&key) else {
            return Err(Error::Missing { key });
        };
        let previous = std::mem::replace(&mut self.entries[index].item.value, value.to_owned());
        Ok(previous)
    }

    pub fn remove(&mut self, key: &str) -> Result<Item, Error> {
        let key = canonicalize_key(key)?;
        let Some(&index) = self.indices.get(&key) else {
            return Err(Error::Missing { key });
        };
        Ok(self.remove_index(index))
    }

    pub fn set_capacity(&mut self, capacity: Option<usize>) -> Vec<Item> {
        self.capacity = capacity;
        let mut evicted = self.trim_to_capacity();
        evicted.sort_by(|a, b| {
            let age_a = self.age_for_key(&a.key).unwrap_or(u128::MAX);
            let age_b = self.age_for_key(&b.key).unwrap_or(u128::MAX);
            // Evicted entries have already been removed; this fallback is corrected below
            // by sorting during trimming in worst-to-best order.
            age_a.cmp(&age_b)
        });
        evicted
    }

    fn age_for_key(&self, key: &str) -> Option<u128> {
        self.indices.get(key).map(|&i| self.entries[i].age)
    }

    pub fn iter(&self) -> std::vec::IntoIter<Item> {
        self.ordered_entries()
            .into_iter()
            .map(|entry| entry.item.clone())
            .collect::<Vec<_>>()
            .into_iter()
    }

    pub fn merge(
        &mut self,
        other: &mut Self,
        policy: CollisionPolicy,
    ) -> Result<MergeReport, Error> {
        let source_order = other.ordered_entries();
        if policy == CollisionPolicy::Reject {
            for entry in &source_order {
                if self.indices.contains_key(&entry.item.key) {
                    return Err(Error::Collision {
                        key: entry.item.key.clone(),
                    });
                }
            }
        }

        let source_items: Vec<Item> = source_order
            .into_iter()
            .map(|entry| entry.item.clone())
            .collect();
        let mut report = MergeReport {
            moved: 0,
            replaced: 0,
            discarded: 0,
            evicted: Vec::new(),
        };

        for item in source_items {
            if let Some(&index) = self.indices.get(&item.key) {
                match policy {
                    CollisionPolicy::KeepSelf => report.discarded += 1,
                    CollisionPolicy::TakeOther => {
                        self.entries[index].item.priority = item.priority;
                        self.entries[index].item.value = item.value;
                        report.replaced += 1;
                    }
                    CollisionPolicy::Reject => unreachable!(),
                }
            } else {
                let age = self.fresh_age();
                self.entries.push(Entry { item, age });
                report.moved += 1;
                self.rebuild_indices();
            }
        }

        other.entries.clear();
        other.indices.clear();
        report.evicted = self.trim_to_capacity();
        Ok(report)
    }
}

impl Default for IndexedHeap {
    fn default() -> Self {
        Self::new()
    }
}