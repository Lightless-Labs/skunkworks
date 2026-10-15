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
    capacity: Option<usize>,
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
            next_age: 0,
            capacity,
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

    fn cmp_entries(a: &Entry, b: &Entry) -> std::cmp::Ordering {
        a.item
            .priority
            .cmp(&b.item.priority)
            .then_with(|| a.item.value.len().cmp(&b.item.value.len()))
            .then_with(|| a.age.cmp(&b.age))
    }

    fn best_index(&self) -> Option<usize> {
        (0..self.entries.len()).min_by(|&a, &b| {
            Self::cmp_entries(&self.entries[a], &self.entries[b])
        })
    }

    fn worst_index(&self) -> Option<usize> {
        (0..self.entries.len()).max_by(|&a, &b| {
            Self::cmp_entries(&self.entries[a], &self.entries[b])
        })
    }

    fn rebuild_indices(&mut self) {
        self.indices.clear();
        for (index, entry) in self.entries.iter().enumerate() {
            self.indices.insert(entry.item.key.clone(), index);
        }
    }

    fn remove_index(&mut self, index: usize) -> Item {
        let removed = self.entries.swap_remove(index);
        self.rebuild_indices();
        removed.item
    }

    fn fresh_age(&mut self) -> u128 {
        if self.next_age == u128::MAX {
            let mut order: Vec<usize> = (0..self.entries.len()).collect();
            order.sort_by(|&a, &b| Self::cmp_entries(&self.entries[a], &self.entries[b]));
            for (age, index) in order.into_iter().enumerate() {
                self.entries[index].age = age as u128;
            }
            self.next_age = self.entries.len() as u128;
        }
        let age = self.next_age;
        self.next_age += 1;
        age
    }

    pub fn push(
        &mut self,
        key: &str,
        priority: i64,
        value: &str,
    ) -> Result<Admission, Error> {
        let key = Self::canonical_key(key)?;
        if self.indices.contains_key(&key) {
            return Err(Error::Duplicate { key });
        }

        let proposed = Item {
            key: key.clone(),
            priority,
            value: value.to_owned(),
        };

        if let Some(limit) = self.capacity {
            if limit == 0 {
                return Ok(Admission::Discarded(proposed));
            }
            if self.entries.len() >= limit {
                let worst = self.worst_index().expect("full nonzero queue");
                let proposed_entry = Entry {
                    item: proposed.clone(),
                    age: self.next_age,
                };
                if Self::cmp_entries(&proposed_entry, &self.entries[worst])
                    != std::cmp::Ordering::Less
                {
                    return Ok(Admission::Discarded(proposed));
                }
                let evicted = self.remove_index(worst);
                let age = self.fresh_age();
                let index = self.entries.len();
                self.entries.push(Entry { item: proposed, age });
                self.indices.insert(key, index);
                return Ok(Admission::Evicted(evicted));
            }
        }

        let age = self.fresh_age();
        let index = self.entries.len();
        self.entries.push(Entry { item: proposed, age });
        self.indices.insert(key, index);
        Ok(Admission::Inserted)
    }

    pub fn peek(&self) -> Option<Item> {
        self.best_index().map(|index| self.entries[index].item.clone())
    }

    pub fn pop(&mut self) -> Option<Item> {
        let index = self.best_index()?;
        Some(self.remove_index(index))
    }

    pub fn get(&self, key: &str) -> Result<Option<Item>, Error> {
        let key = Self::canonical_key(key)?;
        Ok(self
            .indices
            .get(&key)
            .map(|&index| self.entries[index].item.clone()))
    }

    pub fn decrease_key(&mut self, key: &str, priority: i64) -> Result<i64, Error> {
        let key = Self::canonical_key(key)?;
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
        let key = Self::canonical_key(key)?;
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
        let key = Self::canonical_key(key)?;
        let Some(&index) = self.indices.get(&key) else {
            return Err(Error::Missing { key });
        };
        Ok(std::mem::replace(
            &mut self.entries[index].item.value,
            value.to_owned(),
        ))
    }

    pub fn remove(&mut self, key: &str) -> Result<Item, Error> {
        let key = Self::canonical_key(key)?;
        let Some(&index) = self.indices.get(&key) else {
            return Err(Error::Missing { key });
        };
        Ok(self.remove_index(index))
    }

    pub fn set_capacity(&mut self, capacity: Option<usize>) -> Vec<Item> {
        self.capacity = capacity;
        let mut evicted = Vec::new();
        if let Some(limit) = capacity {
            while self.entries.len() > limit {
                let index = self.worst_index().expect("queue exceeds capacity");
                evicted.push(self.remove_index(index));
            }
        }
        evicted
    }

    pub fn iter(&self) -> std::vec::IntoIter<Item> {
        let mut entries: Vec<&Entry> = self.entries.iter().collect();
        entries.sort_by(|a, b| Self::cmp_entries(a, b));
        entries
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
        let source_order: Vec<usize> = {
            let mut order: Vec<usize> = (0..other.entries.len()).collect();
            order.sort_by(|&a, &b| Self::cmp_entries(&other.entries[a], &other.entries[b]));
            order
        };

        if policy == CollisionPolicy::Reject {
            for &index in &source_order {
                let key = &other.entries[index].item.key;
                if self.indices.contains_key(key) {
                    return Err(Error::Collision { key: key.clone() });
                }
            }
        }

        let source_items: Vec<Item> = source_order
            .iter()
            .map(|&index| other.entries[index].item.clone())
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
                    CollisionPolicy::Reject => unreachable!(),
                    CollisionPolicy::KeepSelf => report.discarded += 1,
                    CollisionPolicy::TakeOther => {
                        self.entries[index].item.priority = item.priority;
                        self.entries[index].item.value = item.value;
                        report.replaced += 1;
                    }
                }
            } else {
                let age = self.fresh_age();
                let index = self.entries.len();
                self.entries.push(Entry {
                    item: item.clone(),
                    age,
                });
                self.indices.insert(item.key, index);
                report.moved += 1;
            }
        }

        other.entries.clear();
        other.indices.clear();

        if let Some(limit) = self.capacity {
            while self.entries.len() > limit {
                let index = self.worst_index().expect("queue exceeds capacity");
                report.evicted.push(self.remove_index(index));
            }
        }

        Ok(report)
    }
}