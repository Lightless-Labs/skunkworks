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

        let proposed = Item {
            key,
            priority,
            value: value.to_owned(),
        };

        if self.capacity == Some(0) {
            return Ok(Admission::Discarded(proposed));
        }

        if let Some(limit) = self.capacity {
            if self.entries.len() >= limit {
                let worst_index = self.worst_index();
                let worst = &self.entries[worst_index];
                if compare_item_age(&proposed, self.next_age, &worst.item, worst.age)
                    != Ordering::Less
                {
                    return Ok(Admission::Discarded(proposed));
                }
                let evicted = self.remove_index(worst_index).item;
                self.insert_new(proposed);
                return Ok(Admission::Evicted(evicted));
            }
        }

        self.insert_new(proposed);
        Ok(Admission::Inserted)
    }

    pub fn peek(&self) -> Option<Item> {
        self.best_index().map(|index| self.entries[index].item.clone())
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
        let index = self
            .indices
            .get(&key)
            .copied()
            .ok_or_else(|| Error::Missing { key })?;
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
        let index = self
            .indices
            .get(&key)
            .copied()
            .ok_or_else(|| Error::Missing { key })?;
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
        let index = self
            .indices
            .get(&key)
            .copied()
            .ok_or_else(|| Error::Missing { key })?;
        Ok(std::mem::replace(
            &mut self.entries[index].item.value,
            value.to_owned(),
        ))
    }

    pub fn remove(&mut self, key: &str) -> Result<Item, Error> {
        let key = canonical_key(key)?;
        let index = self
            .indices
            .get(&key)
            .copied()
            .ok_or_else(|| Error::Missing { key })?;
        Ok(self.remove_index(index).item)
    }

    pub fn set_capacity(&mut self, capacity: Option<usize>) -> Vec<Item> {
        self.capacity = capacity;
        let mut evicted = Vec::new();
        while let Some(limit) = self.capacity {
            if self.entries.len() <= limit {
                break;
            }
            let index = self.worst_index();
            evicted.push(self.remove_index(index).item);
        }
        evicted
    }

    pub fn iter(&self) -> std::vec::IntoIter<Item> {
        let mut indices: Vec<usize> = (0..self.entries.len()).collect();
        indices.sort_by(|&a, &b| {
            compare_item_age(
                &self.entries[a].item,
                self.entries[a].age,
                &self.entries[b].item,
                self.entries[b].age,
            )
        });
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
        let mut source_order: Vec<usize> = (0..other.entries.len()).collect();
        source_order.sort_by(|&a, &b| {
            compare_item_age(
                &other.entries[a].item,
                other.entries[a].age,
                &other.entries[b].item,
                other.entries[b].age,
            )
        });

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

        for source_index in source_order {
            let source_item = other.entries[source_index].item.clone();
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
                self.insert_new(source_item);
                report.moved += 1;
            }
        }

        other.entries.clear();
        other.indices.clear();

        while let Some(limit) = self.capacity {
            if self.entries.len() <= limit {
                break;
            }
            let index = self.worst_index();
            report.evicted.push(self.remove_index(index).item);
        }

        Ok(report)
    }

    fn insert_new(&mut self, item: Item) {
        self.rebase_ages_if_needed();
        let age = self.next_age;
        self.next_age += 1;
        let index = self.entries.len();
        self.indices.insert(item.key.clone(), index);
        self.entries.push(Entry { item, age });
    }

    fn rebase_ages_if_needed(&mut self) {
        if self.next_age != u128::MAX {
            return;
        }
        let mut order: Vec<usize> = (0..self.entries.len()).collect();
        order.sort_by_key(|&index| self.entries[index].age);
        for (age, index) in order.into_iter().enumerate() {
            self.entries[index].age = age as u128;
        }
        self.next_age = self.entries.len() as u128;
    }

    fn best_index(&self) -> Option<usize> {
        (0..self.entries.len()).min_by(|&a, &b| {
            compare_item_age(
                &self.entries[a].item,
                self.entries[a].age,
                &self.entries[b].item,
                self.entries[b].age,
            )
        })
    }

    fn worst_index(&self) -> usize {
        (0..self.entries.len())
            .max_by(|&a, &b| {
                compare_item_age(
                    &self.entries[a].item,
                    self.entries[a].age,
                    &self.entries[b].item,
                    self.entries[b].age,
                )
            })
            .expect("worst_index requires a nonempty queue")
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
}

impl Default for IndexedHeap {
    fn default() -> Self {
        Self::new()
    }
}

fn compare_item_age(a: &Item, age_a: u128, b: &Item, age_b: u128) -> Ordering {
    a.priority
        .cmp(&b.priority)
        .then_with(|| a.value.len().cmp(&b.value.len()))
        .then_with(|| age_a.cmp(&age_b))
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