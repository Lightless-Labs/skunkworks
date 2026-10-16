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

#[derive(Debug, Clone)]
struct Entry {
    item: Item,
    age: u128,
}

pub struct IndexedHeap {
    entries: Vec<Entry>,
    indices: HashMap<String, usize>,
    next_age: u128,
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
        Self {
            entries: Vec::new(),
            indices: HashMap::new(),
            next_age: 0,
        }
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    fn better(a: &Entry, b: &Entry) -> bool {
        a.item
            .priority
            .cmp(&b.item.priority)
            .then_with(|| a.item.value.len().cmp(&b.item.value.len()))
            .then_with(|| a.age.cmp(&b.age))
            .is_lt()
    }

    fn rebuild_indices(&mut self) {
        self.indices.clear();
        for (index, entry) in self.entries.iter().enumerate() {
            self.indices.insert(entry.item.key.clone(), index);
        }
    }

    fn sort_entries(&mut self) {
        self.entries.sort_by(|a, b| {
            a.item
                .priority
                .cmp(&b.item.priority)
                .then_with(|| a.item.value.len().cmp(&b.item.value.len()))
                .then_with(|| a.age.cmp(&b.age))
        });
        self.rebuild_indices();
    }

    fn fresh_age(&mut self) -> u128 {
        if self.next_age == u128::MAX {
            let mut order: Vec<(usize, u128)> = self
                .entries
                .iter()
                .enumerate()
                .map(|(index, entry)| (index, entry.age))
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

    pub fn push(&mut self, key: &str, priority: i64, value: &str) -> Result<(), Error> {
        let key = canonicalize(key)?;
        if self.indices.contains_key(&key) {
            return Err(Error::Duplicate { key });
        }

        let age = self.fresh_age();
        self.entries.push(Entry {
            item: Item {
                key,
                priority,
                value: value.to_owned(),
            },
            age,
        });
        self.sort_entries();
        Ok(())
    }

    pub fn peek(&self) -> Option<Item> {
        self.entries.first().map(|entry| entry.item.clone())
    }

    pub fn pop(&mut self) -> Option<Item> {
        if self.entries.is_empty() {
            return None;
        }
        let entry = self.entries.remove(0);
        self.sort_entries();
        Some(entry.item)
    }

    pub fn get(&self, key: &str) -> Result<Option<Item>, Error> {
        let key = canonicalize(key)?;
        Ok(self
            .indices
            .get(&key)
            .map(|&index| self.entries[index].item.clone()))
    }

    pub fn decrease_key(&mut self, key: &str, priority: i64) -> Result<i64, Error> {
        let key = canonicalize(key)?;
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
        self.sort_entries();
        Ok(current)
    }

    pub fn increase_key(&mut self, key: &str, priority: i64) -> Result<i64, Error> {
        let key = canonicalize(key)?;
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
        self.sort_entries();
        Ok(current)
    }

    pub fn replace_value(&mut self, key: &str, value: &str) -> Result<String, Error> {
        let key = canonicalize(key)?;
        let index = *self
            .indices
            .get(&key)
            .ok_or_else(|| Error::Missing { key: key.clone() })?;
        let previous = std::mem::replace(
            &mut self.entries[index].item.value,
            value.to_owned(),
        );
        self.sort_entries();
        Ok(previous)
    }

    pub fn remove(&mut self, key: &str) -> Result<Item, Error> {
        let key = canonicalize(key)?;
        let index = *self
            .indices
            .get(&key)
            .ok_or_else(|| Error::Missing { key: key.clone() })?;
        let entry = self.entries.remove(index);
        self.sort_entries();
        Ok(entry.item)
    }

    pub fn merge(
        &mut self,
        other: &mut Self,
        policy: CollisionPolicy,
    ) -> Result<MergeReport, Error> {
        let mut source: Vec<Entry> = other.entries.clone();
        source.sort_by(|a, b| {
            a.item
                .priority
                .cmp(&b.item.priority)
                .then_with(|| a.item.value.len().cmp(&b.item.value.len()))
                .then_with(|| a.age.cmp(&b.age))
        });

        if policy == CollisionPolicy::Reject {
            for entry in &source {
                if self.indices.contains_key(&entry.item.key) {
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
        };

        for entry in source {
            if let Some(&index) = self.indices.get(&entry.item.key) {
                match policy {
                    CollisionPolicy::Reject => unreachable!(),
                    CollisionPolicy::KeepSelf => report.discarded += 1,
                    CollisionPolicy::TakeOther => {
                        self.entries[index].item.priority = entry.item.priority;
                        self.entries[index].item.value = entry.item.value;
                        report.replaced += 1;
                    }
                }
            } else {
                let age = self.fresh_age();
                self.entries.push(Entry {
                    item: entry.item,
                    age,
                });
                report.moved += 1;
            }
        }

        self.sort_entries();
        other.entries.clear();
        other.indices.clear();
        other.next_age = 0;
        Ok(report)
    }
}