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
    age: u64,
}

pub struct IndexedHeap {
    entries: Vec<Entry>,
    capacity: Option<usize>,
    next_age: u64,
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
        self.entries.is_empty()
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
        if self.entries.iter().any(|entry| entry.item.key == key) {
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

        let age = self.next_age;
        let entry = Entry {
            item: item.clone(),
            age,
        };

        if let Some(limit) = self.capacity {
            if self.entries.len() >= limit {
                let worst = self.worst_index();
                if compare_entries(&entry, &self.entries[worst]).is_lt() {
                    let evicted = self.entries.swap_remove(worst).item;
                    self.entries.push(entry);
                    self.advance_age();
                    return Ok(Admission::Evicted(evicted));
                }
                return Ok(Admission::Discarded(item));
            }
        }

        self.entries.push(entry);
        self.advance_age();
        Ok(Admission::Inserted)
    }

    pub fn peek(&self) -> Option<Item> {
        self.best_index()
            .map(|index| self.entries[index].item.clone())
    }

    pub fn pop(&mut self) -> Option<Item> {
        self.best_index()
            .map(|index| self.entries.swap_remove(index).item)
    }

    pub fn get(&self, key: &str) -> Result<Option<Item>, Error> {
        let key = canonicalize_key(key)?;
        Ok(self
            .entries
            .iter()
            .find(|entry| entry.item.key == key)
            .map(|entry| entry.item.clone()))
    }

    pub fn decrease_key(&mut self, key: &str, priority: i64) -> Result<i64, Error> {
        let key = canonicalize_key(key)?;
        let index = self.find_index(&key)?;
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
        let index = self.find_index(&key)?;
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
        let index = self.find_index(&key)?;
        Ok(std::mem::replace(
            &mut self.entries[index].item.value,
            value.to_owned(),
        ))
    }

    pub fn remove(&mut self, key: &str) -> Result<Item, Error> {
        let key = canonicalize_key(key)?;
        let index = self.find_index(&key)?;
        Ok(self.entries.swap_remove(index).item)
    }

    pub fn merge(
        &mut self,
        other: &mut Self,
        policy: CollisionPolicy,
    ) -> Result<MergeReport, Error> {
        let source_order = other.ordered_indices();
        if policy == CollisionPolicy::Reject {
            for &source_index in &source_order {
                let source_key = &other.entries[source_index].item.key;
                if self.entries.iter().any(|entry| &entry.item.key == source_key) {
                    return Err(Error::Collision {
                        key: source_key.clone(),
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

        for source_index in source_order {
            let source_item = other.entries[source_index].item.clone();
            if let Some(receiver_index) = self
                .entries
                .iter()
                .position(|entry| entry.item.key == source_item.key)
            {
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
                self.entries.push(Entry {
                    item: source_item,
                    age: self.next_age,
                });
                self.advance_age();
                report.moved += 1;
            }
        }

        other.entries.clear();
        report.evicted = self.trim_to_capacity();
        Ok(report)
    }

    pub fn set_capacity(&mut self, capacity: Option<usize>) -> Vec<Item> {
        self.capacity = capacity;
        self.trim_to_capacity()
    }

    pub fn iter(&self) -> std::vec::IntoIter<Item> {
        self.ordered_indices()
            .into_iter()
            .map(|index| self.entries[index].item.clone())
            .collect::<Vec<_>>()
            .into_iter()
    }

    fn advance_age(&mut self) {
        if self.next_age == u64::MAX {
            let mut order: Vec<usize> = (0..self.entries.len()).collect();
            order.sort_by_key(|&index| self.entries[index].age);
            for (age, index) in order.into_iter().enumerate() {
                self.entries[index].age = age as u64;
            }
            self.next_age = self.entries.len() as u64;
        } else {
            self.next_age += 1;
        }
    }

    fn find_index(&self, key: &str) -> Result<usize, Error> {
        self.entries
            .iter()
            .position(|entry| entry.item.key == key)
            .ok_or_else(|| Error::Missing {
                key: key.to_owned(),
            })
    }

    fn best_index(&self) -> Option<usize> {
        self.entries
            .iter()
            .enumerate()
            .min_by(|(_, left), (_, right)| compare_entries(left, right))
            .map(|(index, _)| index)
    }

    fn worst_index(&self) -> usize {
        self.entries
            .iter()
            .enumerate()
            .max_by(|(_, left), (_, right)| compare_entries(left, right))
            .map(|(index, _)| index)
            .expect("worst_index requires a nonempty queue")
    }

    fn ordered_indices(&self) -> Vec<usize> {
        let mut indices: Vec<usize> = (0..self.entries.len()).collect();
        indices.sort_by(|&left, &right| {
            compare_entries(&self.entries[left], &self.entries[right])
        });
        indices
    }

    fn trim_to_capacity(&mut self) -> Vec<Item> {
        let Some(limit) = self.capacity else {
            return Vec::new();
        };

        let excess = self.entries.len().saturating_sub(limit);
        if excess == 0 {
            return Vec::new();
        }

        let mut indices = self.ordered_indices();
        indices.reverse();
        let evicted_indices = &indices[..excess];
        let evicted_set: std::collections::HashSet<usize> =
            evicted_indices.iter().copied().collect();

        let mut evicted: Vec<Item> = evicted_indices
            .iter()
            .map(|&index| self.entries[index].item.clone())
            .collect();
        for index in (0..self.entries.len()).rev() {
            if evicted_set.contains(&index) {
                self.entries.swap_remove(index);
            }
        }

        evicted.shrink_to_fit();
        evicted
    }
}

fn compare_entries(left: &Entry, right: &Entry) -> std::cmp::Ordering {
    left.item
        .priority
        .cmp(&right.item.priority)
        .then_with(|| left.item.value.len().cmp(&right.item.value.len()))
        .then_with(|| left.age.cmp(&right.age))
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