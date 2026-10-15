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

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Admission {
    Inserted,
    Evicted(Item),
    Discarded(Item),
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

#[derive(Clone)]
struct Entry {
    item: Item,
    age: u128,
}

pub struct IndexedHeap {
    entries: Vec<Entry>,
    positions: HashMap<String, usize>,
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
            positions: HashMap::new(),
            capacity,
            next_age: 0,
        }
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    pub fn capacity(&self) -> Option<usize> {
        self.capacity
    }

    fn validate_key(input: &str) -> Result<String, Error> {
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

    fn compare(a: &Entry, b: &Entry) -> Ordering {
        a.item
            .priority
            .cmp(&b.item.priority)
            .then_with(|| a.item.value.len().cmp(&b.item.value.len()))
            .then_with(|| a.age.cmp(&b.age))
    }

    fn best_index(&self) -> Option<usize> {
        (0..self.entries.len()).min_by(|&a, &b| Self::compare(&self.entries[a], &self.entries[b]))
    }

    fn worst_index(&self) -> Option<usize> {
        (0..self.entries.len()).max_by(|&a, &b| Self::compare(&self.entries[a], &self.entries[b]))
    }

    fn remove_at(&mut self, index: usize) -> Entry {
        let removed = self.entries.remove(index);
        self.positions.remove(&removed.item.key);
        for i in index..self.entries.len() {
            self.positions.insert(self.entries[i].item.key.clone(), i);
        }
        removed
    }

    fn insert_entry(&mut self, entry: Entry) {
        let index = self.entries.len();
        self.positions.insert(entry.item.key.clone(), index);
        self.entries.push(entry);
    }

    fn fresh_age(&mut self) -> u128 {
        let age = self.next_age;
        self.next_age = self.next_age.wrapping_add(1);
        age
    }

    fn ordered_indices(&self) -> Vec<usize> {
        let mut indices: Vec<usize> = (0..self.entries.len()).collect();
        indices.sort_by(|&a, &b| Self::compare(&self.entries[a], &self.entries[b]));
        indices
    }

    fn trim_to_capacity(&mut self) -> Vec<Item> {
        let mut evicted = Vec::new();
        if let Some(capacity) = self.capacity {
            while self.entries.len() > capacity {
                let index = self.worst_index().expect("excess entries imply a nonempty queue");
                evicted.push(self.remove_at(index).item);
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
        let key = Self::validate_key(key)?;
        if self.positions.contains_key(&key) {
            return Err(Error::Duplicate { key });
        }

        let item = Item {
            key,
            priority,
            value: value.to_owned(),
        };

        if self.capacity == Some(0) {
            return Ok(Admission::Discarded(item));
        }

        if let Some(capacity) = self.capacity {
            if self.entries.len() >= capacity {
                let proposed = Entry {
                    item: item.clone(),
                    age: self.next_age,
                };
                let worst = self.worst_index().expect("full nonzero queue is nonempty");
                if Self::compare(&proposed, &self.entries[worst]) != Ordering::Less {
                    return Ok(Admission::Discarded(item));
                }
                let evicted = self.remove_at(worst).item;
                let age = self.fresh_age();
                self.insert_entry(Entry { item, age });
                return Ok(Admission::Evicted(evicted));
            }
        }

        let age = self.fresh_age();
        self.insert_entry(Entry { item, age });
        Ok(Admission::Inserted)
    }

    pub fn peek(&self) -> Option<Item> {
        self.best_index().map(|i| self.entries[i].item.clone())
    }

    pub fn pop(&mut self) -> Option<Item> {
        self.best_index().map(|i| self.remove_at(i).item)
    }

    pub fn get(&self, key: &str) -> Result<Option<Item>, Error> {
        let key = Self::validate_key(key)?;
        Ok(self.positions.get(&key).map(|&i| self.entries[i].item.clone()))
    }

    pub fn decrease_key(&mut self, key: &str, priority: i64) -> Result<i64, Error> {
        let key = Self::validate_key(key)?;
        let Some(&index) = self.positions.get(&key) else {
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
        let key = Self::validate_key(key)?;
        let Some(&index) = self.positions.get(&key) else {
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
        let key = Self::validate_key(key)?;
        let Some(&index) = self.positions.get(&key) else {
            return Err(Error::Missing { key });
        };
        Ok(std::mem::replace(
            &mut self.entries[index].item.value,
            value.to_owned(),
        ))
    }

    pub fn remove(&mut self, key: &str) -> Result<Item, Error> {
        let key = Self::validate_key(key)?;
        let Some(&index) = self.positions.get(&key) else {
            return Err(Error::Missing { key });
        };
        Ok(self.remove_at(index).item)
    }

    pub fn set_capacity(&mut self, capacity: Option<usize>) -> Vec<Item> {
        self.capacity = capacity;
        let mut evicted = self.trim_to_capacity();
        evicted.reverse();
        evicted
    }

    pub fn iter(&self) -> std::vec::IntoIter<Item> {
        self.ordered_indices()
            .into_iter()
            .map(|i| self.entries[i].item.clone())
            .collect::<Vec<_>>()
            .into_iter()
    }

    pub fn merge(
        &mut self,
        other: &mut Self,
        policy: CollisionPolicy,
    ) -> Result<MergeReport, Error> {
        let source_order: Vec<Entry> = other
            .ordered_indices()
            .into_iter()
            .map(|i| other.entries[i].clone())
            .collect();

        if policy == CollisionPolicy::Reject {
            for entry in &source_order {
                if self.positions.contains_key(&entry.item.key) {
                    return Err(Error::Collision {
                        key: entry.item.key.clone(),
                    });
                }
            }
        }

        let mut moved = 0;
        let mut replaced = 0;
        let mut discarded = 0;

        for source in source_order {
            if let Some(&index) = self.positions.get(&source.item.key) {
                match policy {
                    CollisionPolicy::KeepSelf => discarded += 1,
                    CollisionPolicy::TakeOther => {
                        self.entries[index].item.priority = source.item.priority;
                        self.entries[index].item.value = source.item.value;
                        replaced += 1;
                    }
                    CollisionPolicy::Reject => unreachable!(),
                }
            } else {
                let age = self.fresh_age();
                self.insert_entry(Entry {
                    item: source.item,
                    age,
                });
                moved += 1;
            }
        }

        other.entries.clear();
        other.positions.clear();

        let evicted = self.trim_to_capacity();
        Ok(MergeReport {
            moved,
            replaced,
            discarded,
            evicted,
        })
    }
}

impl Default for IndexedHeap {
    fn default() -> Self {
        Self::new()
    }
}

fn main() {}