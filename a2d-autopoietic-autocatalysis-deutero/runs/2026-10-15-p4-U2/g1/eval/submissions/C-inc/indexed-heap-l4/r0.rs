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
    indices: HashMap<String, usize>,
    capacity: Option<usize>,
    next_age: u128,
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

    fn normalize_key(key: &str) -> Result<String, Error> {
        let bytes = key.as_bytes();
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

        Ok(key[start..end].to_ascii_lowercase())
    }

    fn compare_entries(a: &Entry, b: &Entry) -> Ordering {
        a.item
            .priority
            .cmp(&b.item.priority)
            .then_with(|| a.item.value.len().cmp(&b.item.value.len()))
            .then_with(|| a.age.cmp(&b.age))
    }

    fn compare_items(
        priority_a: i64,
        value_a: &str,
        age_a: u128,
        priority_b: i64,
        value_b: &str,
        age_b: u128,
    ) -> Ordering {
        priority_a
            .cmp(&priority_b)
            .then_with(|| value_a.len().cmp(&value_b.len()))
            .then_with(|| age_a.cmp(&age_b))
    }

    fn swap_slots(&mut self, a: usize, b: usize) {
        self.entries.swap(a, b);
        let key_a = self.entries[a].item.key.clone();
        let key_b = self.entries[b].item.key.clone();
        self.indices.insert(key_a, a);
        self.indices.insert(key_b, b);
    }

    fn sift_up(&mut self, mut index: usize) {
        while index > 0 {
            let parent = (index - 1) / 2;
            if Self::compare_entries(&self.entries[index], &self.entries[parent]) != Ordering::Less {
                break;
            }
            self.swap_slots(index, parent);
            index = parent;
        }
    }

    fn sift_down(&mut self, mut index: usize) {
        loop {
            let left = index * 2 + 1;
            if left >= self.entries.len() {
                break;
            }
            let right = left + 1;
            let mut best = left;
            if right < self.entries.len()
                && Self::compare_entries(&self.entries[right], &self.entries[left]) == Ordering::Less
            {
                best = right;
            }
            if Self::compare_entries(&self.entries[best], &self.entries[index]) != Ordering::Less {
                break;
            }
            self.swap_slots(index, best);
            index = best;
        }
    }

    fn repair(&mut self, index: usize) {
        if index > 0
            && Self::compare_entries(&self.entries[index], &self.entries[(index - 1) / 2])
                == Ordering::Less
        {
            self.sift_up(index);
        } else {
            self.sift_down(index);
        }
    }

    fn remove_at(&mut self, index: usize) -> Entry {
        let removed = self.entries.swap_remove(index);
        self.indices.remove(&removed.item.key);
        if index < self.entries.len() {
            let moved_key = self.entries[index].item.key.clone();
            self.indices.insert(moved_key, index);
            self.repair(index);
        }
        removed
    }

    fn append_entry(&mut self, entry: Entry) {
        let index = self.entries.len();
        self.indices.insert(entry.item.key.clone(), index);
        self.entries.push(entry);
        self.sift_up(index);
    }

    fn sorted_indices(&self) -> Vec<usize> {
        let mut indices: Vec<usize> = (0..self.entries.len()).collect();
        indices.sort_by(|&a, &b| Self::compare_entries(&self.entries[a], &self.entries[b]));
        indices
    }

    fn worst_to_best(&self) -> Vec<Item> {
        let mut indices = self.sorted_indices();
        indices.reverse();
        indices
            .into_iter()
            .map(|index| self.entries[index].item.clone())
            .collect()
    }

    pub fn push(
        &mut self,
        key: &str,
        priority: i64,
        value: &str,
    ) -> Result<Admission, Error> {
        let key = Self::normalize_key(key)?;
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

        if let Some(capacity) = self.capacity {
            if self.entries.len() >= capacity {
                let worst_index = self
                    .entries
                    .iter()
                    .enumerate()
                    .max_by(|(_, a), (_, b)| Self::compare_entries(a, b))
                    .map(|(index, _)| index)
                    .expect("a full nonzero-capacity heap is nonempty");
                let worst = &self.entries[worst_index];
                if Self::compare_items(
                    priority,
                    &proposed.value,
                    self.next_age,
                    worst.item.priority,
                    &worst.item.value,
                    worst.age,
                ) != Ordering::Less
                {
                    return Ok(Admission::Discarded(proposed));
                }
                let removed = self.remove_at(worst_index).item;
                let age = self.take_age();
                self.append_entry(Entry {
                    item: proposed,
                    age,
                });
                return Ok(Admission::Evicted(removed));
            }
        }

        let age = self.take_age();
        self.append_entry(Entry {
            item: proposed,
            age,
        });
        Ok(Admission::Inserted)
    }

    fn take_age(&mut self) -> u128 {
        let age = self.next_age;
        self.next_age = self.next_age.wrapping_add(1);
        age
    }

    pub fn peek(&self) -> Option<Item> {
        self.entries.first().map(|entry| entry.item.clone())
    }

    pub fn pop(&mut self) -> Option<Item> {
        if self.entries.is_empty() {
            None
        } else {
            Some(self.remove_at(0).item)
        }
    }

    pub fn get(&self, key: &str) -> Result<Option<Item>, Error> {
        let key = Self::normalize_key(key)?;
        Ok(self
            .indices
            .get(&key)
            .map(|&index| self.entries[index].item.clone()))
    }

    pub fn decrease_key(&mut self, key: &str, priority: i64) -> Result<i64, Error> {
        let key = Self::normalize_key(key)?;
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
        self.repair(index);
        Ok(current)
    }

    pub fn increase_key(&mut self, key: &str, priority: i64) -> Result<i64, Error> {
        let key = Self::normalize_key(key)?;
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
        self.repair(index);
        Ok(current)
    }

    pub fn replace_value(&mut self, key: &str, value: &str) -> Result<String, Error> {
        let key = Self::normalize_key(key)?;
        let index = *self
            .indices
            .get(&key)
            .ok_or_else(|| Error::Missing { key: key.clone() })?;
        let previous = std::mem::replace(&mut self.entries[index].item.value, value.to_owned());
        self.repair(index);
        Ok(previous)
    }

    pub fn remove(&mut self, key: &str) -> Result<Item, Error> {
        let key = Self::normalize_key(key)?;
        let index = *self
            .indices
            .get(&key)
            .ok_or_else(|| Error::Missing { key: key.clone() })?;
        Ok(self.remove_at(index).item)
    }

    pub fn set_capacity(&mut self, capacity: Option<usize>) -> Vec<Item> {
        self.capacity = capacity;
        let mut evicted = Vec::new();
        while let Some(limit) = self.capacity {
            if self.entries.len() <= limit {
                break;
            }
            let worst_index = self
                .entries
                .iter()
                .enumerate()
                .max_by(|(_, a), (_, b)| Self::compare_entries(a, b))
                .map(|(index, _)| index)
                .expect("excess entries imply a nonempty heap");
            evicted.push(self.remove_at(worst_index).item);
        }
        evicted
    }

    pub fn iter(&self) -> std::vec::IntoIter<Item> {
        self.sorted_indices()
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
        let source_order = other.sorted_indices();

        if policy == CollisionPolicy::Reject {
            for &index in &source_order {
                let key = &other.entries[index].item.key;
                if self.indices.contains_key(key) {
                    return Err(Error::Collision { key: key.clone() });
                }
            }
        }

        let mut moved = 0;
        let mut replaced = 0;
        let mut discarded = 0;

        for index in source_order {
            let source_entry = other.entries[index].clone();
            if let Some(&receiver_index) = self.indices.get(&source_entry.item.key) {
                match policy {
                    CollisionPolicy::KeepSelf => discarded += 1,
                    CollisionPolicy::TakeOther => {
                        self.entries[receiver_index].item.priority = source_entry.item.priority;
                        self.entries[receiver_index].item.value = source_entry.item.value;
                        self.repair(receiver_index);
                        replaced += 1;
                    }
                    CollisionPolicy::Reject => unreachable!(),
                }
            } else {
                moved += 1;
                let age = self.take_age();
                self.append_entry(Entry {
                    item: source_entry.item,
                    age,
                });
            }
        }

        other.entries.clear();
        other.indices.clear();

        let evicted = self.set_capacity(self.capacity);
        Ok(MergeReport {
            moved,
            replaced,
            discarded,
            evicted,
        })
    }
}

fn main() {}