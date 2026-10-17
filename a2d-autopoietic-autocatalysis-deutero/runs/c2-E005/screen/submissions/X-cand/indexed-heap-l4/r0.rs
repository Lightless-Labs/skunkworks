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
    capacity: Option<usize>,
    next_age: u128,
}

impl IndexedHeap {
    pub fn new() -> Self {
        Self {
            entries: Vec::new(),
            capacity: None,
            next_age: 0,
        }
    }

    pub fn with_capacity(capacity: usize) -> Self {
        Self {
            entries: Vec::new(),
            capacity: Some(capacity),
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
                byte.is_ascii_alphanumeric() || byte == b'-'
            };
            if !valid {
                return Err(Error::InvalidKey { at: start + offset });
            }
        }
        Ok(input[start..end].to_ascii_lowercase())
    }

    fn compare(a: &Entry, b: &Entry) -> std::cmp::Ordering {
        a.item
            .priority
            .cmp(&b.item.priority)
            .then_with(|| a.item.value.len().cmp(&b.item.value.len()))
            .then_with(|| a.age.cmp(&b.age))
    }

    fn sorted_indices(&self) -> Vec<usize> {
        let mut indices: Vec<usize> = (0..self.entries.len()).collect();
        indices.sort_by(|&a, &b| Self::compare(&self.entries[a], &self.entries[b]));
        indices
    }

    fn find_index(&self, key: &str) -> Option<usize> {
        self.entries.iter().position(|entry| entry.item.key == key)
    }

    fn fresh_age(&mut self) -> u128 {
        if self.next_age == u128::MAX {
            let mut indices: Vec<usize> = (0..self.entries.len()).collect();
            indices.sort_by_key(|&i| self.entries[i].age);
            for (age, index) in indices.into_iter().enumerate() {
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
        if self.find_index(&key).is_some() {
            return Err(Error::Duplicate { key });
        }

        let item = Item {
            key,
            priority,
            value: value.to_owned(),
        };
        if let Some(limit) = self.capacity {
            if limit == 0 {
                return Ok(Admission::Discarded(item));
            }
            if self.entries.len() >= limit {
                let worst = (0..self.entries.len())
                    .max_by(|&a, &b| Self::compare(&self.entries[a], &self.entries[b]))
                    .expect("full nonzero queue has an entry");
                let proposal = Entry {
                    item: item.clone(),
                    age: self.next_age,
                };
                if Self::compare(&proposal, &self.entries[worst]).is_lt() {
                    let evicted = self.entries.remove(worst).item;
                    let age = self.fresh_age();
                    self.entries.push(Entry { item, age });
                    return Ok(Admission::Evicted(evicted));
                }
                return Ok(Admission::Discarded(item));
            }
        }

        let age = self.fresh_age();
        self.entries.push(Entry { item, age });
        Ok(Admission::Inserted)
    }

    pub fn peek(&self) -> Option<Item> {
        self.sorted_indices()
            .first()
            .map(|&i| self.entries[i].item.clone())
    }

    pub fn pop(&mut self) -> Option<Item> {
        let index = self
            .sorted_indices()
            .first()
            .copied()?;
        Some(self.entries.remove(index).item)
    }

    pub fn get(&self, key: &str) -> Result<Option<Item>, Error> {
        let key = Self::canonical_key(key)?;
        Ok(self.find_index(&key).map(|i| self.entries[i].item.clone()))
    }

    pub fn decrease_key(&mut self, key: &str, priority: i64) -> Result<i64, Error> {
        let key = Self::canonical_key(key)?;
        let index = self
            .find_index(&key)
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
        let key = Self::canonical_key(key)?;
        let index = self
            .find_index(&key)
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
        let key = Self::canonical_key(key)?;
        let index = self
            .find_index(&key)
            .ok_or_else(|| Error::Missing { key: key.clone() })?;
        let previous = std::mem::replace(&mut self.entries[index].item.value, value.to_owned());
        Ok(previous)
    }

    pub fn remove(&mut self, key: &str) -> Result<Item, Error> {
        let key = Self::canonical_key(key)?;
        let index = self
            .find_index(&key)
            .ok_or_else(|| Error::Missing { key: key.clone() })?;
        Ok(self.entries.remove(index).item)
    }

    fn trim_to_capacity(&mut self) -> Vec<Item> {
        let Some(limit) = self.capacity else {
            return Vec::new();
        };
        let mut evicted = Vec::new();
        while self.entries.len() > limit {
            let worst = (0..self.entries.len())
                .max_by(|&a, &b| Self::compare(&self.entries[a], &self.entries[b]))
                .expect("excess entries imply a nonempty queue");
            evicted.push(self.entries.remove(worst).item);
        }
        evicted
    }

    pub fn set_capacity(&mut self, capacity: Option<usize>) -> Vec<Item> {
        self.capacity = capacity;
        self.trim_to_capacity()
    }

    pub fn iter(&self) -> std::vec::IntoIter<Item> {
        self.sorted_indices()
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
        let source_order = other.sorted_indices();
        if policy == CollisionPolicy::Reject {
            for &i in &source_order {
                let key = &other.entries[i].item.key;
                if self.find_index(key).is_some() {
                    return Err(Error::Collision { key: key.clone() });
                }
            }
        }

        let source_items: Vec<Item> = source_order
            .into_iter()
            .map(|i| other.entries[i].item.clone())
            .collect();

        let mut report = MergeReport {
            moved: 0,
            replaced: 0,
            discarded: 0,
            evicted: Vec::new(),
        };

        for item in source_items {
            if let Some(index) = self.find_index(&item.key) {
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
            }
        }

        other.entries.clear();
        report.evicted = self.trim_to_capacity();
        Ok(report)
    }
}