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

impl IndexedHeap {
    pub fn new() -> Self {
        Self::with_capacity_option(None)
    }

    pub fn with_capacity(capacity: usize) -> Self {
        Self::with_capacity_option(Some(capacity))
    }

    fn with_capacity_option(capacity: Option<usize>) -> Self {
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

    fn compare_entries(a: &Entry, b: &Entry) -> Ordering {
        a.item
            .priority
            .cmp(&b.item.priority)
            .then_with(|| a.item.value.len().cmp(&b.item.value.len()))
            .then_with(|| a.age.cmp(&b.age))
    }

    fn ordered_indices(&self) -> Vec<usize> {
        let mut indices: Vec<usize> = (0..self.entries.len()).collect();
        indices.sort_by(|&a, &b| {
            Self::compare_entries(&self.entries[a], &self.entries[b])
        });
        indices
    }

    fn rebuild_indices(&mut self) {
        self.indices.clear();
        for (index, entry) in self.entries.iter().enumerate() {
            self.indices.insert(entry.item.key.clone(), index);
        }
    }

    fn fresh_age(&mut self) -> u128 {
        if self.next_age == u128::MAX {
            let mut by_age: Vec<usize> = (0..self.entries.len()).collect();
            by_age.sort_by_key(|&i| self.entries[i].age);
            for (age, index) in by_age.into_iter().enumerate() {
                self.entries[index].age = age as u128;
            }
            self.next_age = self.entries.len() as u128;
        }
        let age = self.next_age;
        self.next_age += 1;
        age
    }

    fn worst_index(&self) -> Option<usize> {
        self.entries
            .iter()
            .enumerate()
            .max_by(|(_, a), (_, b)| Self::compare_entries(a, b))
            .map(|(index, _)| index)
    }

    fn remove_at(&mut self, index: usize) -> Item {
        let removed = self.entries.swap_remove(index).item;
        self.rebuild_indices();
        removed
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
            key,
            priority,
            value: value.to_owned(),
        };

        if let Some(limit) = self.capacity {
            if limit == 0 {
                return Ok(Admission::Discarded(proposed));
            }
            if self.entries.len() >= limit {
                let worst_index = self.worst_index().expect("a full nonzero queue is nonempty");
                let worst = &self.entries[worst_index];
                let proposed_is_better = proposed.priority < worst.item.priority
                    || (proposed.priority == worst.item.priority
                        && proposed.value.len() < worst.item.value.len());

                if !proposed_is_better {
                    return Ok(Admission::Discarded(proposed));
                }

                let evicted = self.remove_at(worst_index);
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
        self.ordered_indices()
            .first()
            .map(|&index| self.entries[index].item.clone())
    }

    pub fn pop(&mut self) -> Option<Item> {
        let index = self.ordered_indices().first().copied()?;
        Some(self.remove_at(index))
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
        let key = canonicalize_key(key)?;
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
        let key = canonicalize_key(key)?;
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
        let key = canonicalize_key(key)?;
        let index = *self
            .indices
            .get(&key)
            .ok_or_else(|| Error::Missing { key: key.clone() })?;
        Ok(self.remove_at(index))
    }

    pub fn set_capacity(&mut self, capacity: Option<usize>) -> Vec<Item> {
        self.capacity = capacity;
        let mut evicted = Vec::new();
        if let Some(limit) = capacity {
            while self.entries.len() > limit {
                let index = self.worst_index().expect("excess entries imply nonempty queue");
                evicted.push(self.remove_at(index));
            }
        }
        evicted
    }

    pub fn iter(&self) -> std::vec::IntoIter<Item> {
        self.ordered_indices()
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
        let source_order = other.ordered_indices();

        if policy == CollisionPolicy::Reject {
            for &index in &source_order {
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

        for index in source_order {
            let source_item = other.entries[index].item.clone();
            if let Some(&receiver_index) = self.indices.get(&source_item.key) {
                match policy {
                    CollisionPolicy::KeepSelf => report.discarded += 1,
                    CollisionPolicy::TakeOther => {
                        self.entries[receiver_index].item.priority = source_item.priority;
                        self.entries[receiver_index].item.value = source_item.value;
                        report.replaced += 1;
                    }
                    CollisionPolicy::Reject => unreachable!(),
                }
            } else {
                let age = self.fresh_age();
                self.entries.push(Entry {
                    item: source_item,
                    age,
                });
                self.indices
                    .insert(self.entries.last().unwrap().item.key.clone(), self.entries.len() - 1);
                report.moved += 1;
            }
        }

        other.entries.clear();
        other.indices.clear();
        other.next_age = 0;

        if let Some(limit) = self.capacity {
            while self.entries.len() > limit {
                let index = self.worst_index().expect("excess entries imply nonempty queue");
                report.evicted.push(self.remove_at(index));
            }
        }

        self.rebuild_indices();
        Ok(report)
    }
}

impl Default for IndexedHeap {
    fn default() -> Self {
        Self::new()
    }
}

fn main() {}