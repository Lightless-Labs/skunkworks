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

#[derive(Debug, Clone)]
pub struct IndexedHeap {
    entries: Vec<Entry>,
    indices: HashMap<String, usize>,
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

    fn cmp_entries(a: &Entry, b: &Entry) -> std::cmp::Ordering {
        a.item
            .priority
            .cmp(&b.item.priority)
            .then_with(|| a.item.value.len().cmp(&b.item.value.len()))
            .then_with(|| a.age.cmp(&b.age))
    }

    fn best_index(&self) -> Option<usize> {
        self.entries
            .iter()
            .enumerate()
            .min_by(|(_, a), (_, b)| Self::cmp_entries(a, b))
            .map(|(i, _)| i)
    }

    fn rebuild_indices(&mut self) {
        self.indices.clear();
        for (i, entry) in self.entries.iter().enumerate() {
            self.indices.insert(entry.item.key.clone(), i);
        }
    }

    fn fresh_age(&mut self) -> u128 {
        if self.next_age == u128::MAX {
            let mut order: Vec<usize> = (0..self.entries.len()).collect();
            order.sort_by_key(|&i| self.entries[i].age);
            for (age, index) in order.into_iter().enumerate() {
                self.entries[index].age = age as u128;
            }
            self.next_age = self.entries.len() as u128;
        }
        let age = self.next_age;
        self.next_age += 1;
        age
    }

    fn remove_at(&mut self, index: usize) -> Entry {
        let removed = self.entries.swap_remove(index);
        self.indices.remove(&removed.item.key);
        if index < self.entries.len() {
            self.indices
                .insert(self.entries[index].item.key.clone(), index);
        }
        removed
    }

    pub fn push(&mut self, key: &str, priority: i64, value: &str) -> Result<(), Error> {
        let key = canonicalize_key(key)?;
        if self.indices.contains_key(&key) {
            return Err(Error::Duplicate { key });
        }

        let age = self.fresh_age();
        let index = self.entries.len();
        self.entries.push(Entry {
            item: Item {
                key: key.clone(),
                priority,
                value: value.to_owned(),
            },
            age,
        });
        self.indices.insert(key, index);
        Ok(())
    }

    pub fn peek(&self) -> Option<Item> {
        self.best_index().map(|i| self.entries[i].item.clone())
    }

    pub fn pop(&mut self) -> Option<Item> {
        let index = self.best_index()?;
        Some(self.remove_at(index).item)
    }

    pub fn get(&self, key: &str) -> Result<Option<Item>, Error> {
        let key = canonicalize_key(key)?;
        Ok(self.indices.get(&key).map(|&i| self.entries[i].item.clone()))
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
        Ok(std::mem::replace(
            &mut self.entries[index].item.value,
            value.to_owned(),
        ))
    }

    pub fn remove(&mut self, key: &str) -> Result<Item, Error> {
        let key = canonicalize_key(key)?;
        let Some(&index) = self.indices.get(&key) else {
            return Err(Error::Missing { key });
        };
        Ok(self.remove_at(index).item)
    }

    pub fn merge(
        &mut self,
        other: &mut Self,
        policy: CollisionPolicy,
    ) -> Result<MergeReport, Error> {
        let mut source_order: Vec<usize> = (0..other.entries.len()).collect();
        source_order.sort_by(|&a, &b| Self::cmp_entries(&other.entries[a], &other.entries[b]));

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
        };

        for index in source_order {
            let source = &other.entries[index].item;
            if let Some(&receiver_index) = self.indices.get(&source.key) {
                match policy {
                    CollisionPolicy::KeepSelf => report.discarded += 1,
                    CollisionPolicy::TakeOther => {
                        self.entries[receiver_index].item.priority = source.priority;
                        self.entries[receiver_index].item.value = source.value.clone();
                        report.replaced += 1;
                    }
                    CollisionPolicy::Reject => unreachable!(),
                }
            } else {
                let age = self.fresh_age();
                let item = source.clone();
                let receiver_index = self.entries.len();
                self.entries.push(Entry {
                    item: item.clone(),
                    age,
                });
                self.indices.insert(item.key.clone(), receiver_index);
                report.moved += 1;
            }
        }

        other.entries.clear();
        other.indices.clear();
        other.next_age = 0;
        Ok(report)
    }
}