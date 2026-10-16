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

#[derive(Clone)]
struct Entry {
    item: Item,
    age: u128,
}

pub struct IndexedHeap {
    heap: Vec<Entry>,
    indices: HashMap<String, usize>,
    next_age: u128,
}

impl IndexedHeap {
    pub fn new() -> Self {
        Self {
            heap: Vec::new(),
            indices: HashMap::new(),
            next_age: 0,
        }
    }

    pub fn len(&self) -> usize {
        self.heap.len()
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    pub fn push(&mut self, key: &str, priority: i64, value: &str) -> Result<(), Error> {
        let key = canonicalize(key)?;
        if self.indices.contains_key(&key) {
            return Err(Error::Duplicate { key });
        }

        self.ensure_age_space();
        let age = self.next_age;
        self.next_age += 1;
        let index = self.heap.len();
        self.heap.push(Entry {
            item: Item {
                key: key.clone(),
                priority,
                value: value.to_owned(),
            },
            age,
        });
        self.indices.insert(key, index);
        self.sift_up(index);
        Ok(())
    }

    pub fn peek(&self) -> Option<Item> {
        self.heap.first().map(|entry| entry.item.clone())
    }

    pub fn pop(&mut self) -> Option<Item> {
        if self.heap.is_empty() {
            return None;
        }
        Some(self.remove_at(0).item)
    }

    pub fn get(&self, key: &str) -> Result<Option<Item>, Error> {
        let key = canonicalize(key)?;
        Ok(self.indices.get(&key).map(|&index| self.heap[index].item.clone()))
    }

    pub fn decrease_key(&mut self, key: &str, priority: i64) -> Result<i64, Error> {
        let key = canonicalize(key)?;
        let index = *self.indices.get(&key).ok_or_else(|| Error::Missing {
            key: key.clone(),
        })?;
        let current = self.heap[index].item.priority;
        if priority >= current {
            return Err(Error::WrongDirection {
                current,
                requested: priority,
            });
        }
        self.heap[index].item.priority = priority;
        self.repair(index);
        Ok(current)
    }

    pub fn increase_key(&mut self, key: &str, priority: i64) -> Result<i64, Error> {
        let key = canonicalize(key)?;
        let index = *self.indices.get(&key).ok_or_else(|| Error::Missing {
            key: key.clone(),
        })?;
        let current = self.heap[index].item.priority;
        if priority <= current {
            return Err(Error::WrongDirection {
                current,
                requested: priority,
            });
        }
        self.heap[index].item.priority = priority;
        self.repair(index);
        Ok(current)
    }

    pub fn replace_value(&mut self, key: &str, value: &str) -> Result<String, Error> {
        let key = canonicalize(key)?;
        let index = *self.indices.get(&key).ok_or_else(|| Error::Missing {
            key: key.clone(),
        })?;
        let old = std::mem::replace(&mut self.heap[index].item.value, value.to_owned());
        self.repair(index);
        Ok(old)
    }

    pub fn remove(&mut self, key: &str) -> Result<Item, Error> {
        let key = canonicalize(key)?;
        let index = *self.indices.get(&key).ok_or_else(|| Error::Missing {
            key: key.clone(),
        })?;
        Ok(self.remove_at(index).item)
    }

    pub fn merge(
        &mut self,
        other: &mut Self,
        policy: CollisionPolicy,
    ) -> Result<MergeReport, Error> {
        let source_order: Vec<Entry> = {
            let mut remaining = other.heap.clone();
            let mut ordered = Vec::with_capacity(remaining.len());
            while !remaining.is_empty() {
                let best = remaining
                    .iter()
                    .enumerate()
                    .min_by(|(_, a), (_, b)| compare_entries(a, b))
                    .map(|(i, _)| i)
                    .unwrap();
                ordered.push(remaining.swap_remove(best));
            }
            ordered
        };

        if policy == CollisionPolicy::Reject {
            for entry in &source_order {
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

        for entry in source_order {
            if let Some(&index) = self.indices.get(&entry.item.key) {
                match policy {
                    CollisionPolicy::KeepSelf => report.discarded += 1,
                    CollisionPolicy::TakeOther => {
                        self.heap[index].item.priority = entry.item.priority;
                        self.heap[index].item.value = entry.item.value;
                        self.repair(index);
                        report.replaced += 1;
                    }
                    CollisionPolicy::Reject => unreachable!(),
                }
            } else {
                self.ensure_age_space();
                let index = self.heap.len();
                self.heap.push(Entry {
                    item: entry.item,
                    age: self.next_age,
                });
                self.next_age += 1;
                self.indices.insert(self.heap[index].item.key.clone(), index);
                self.sift_up(index);
                report.moved += 1;
            }
        }

        other.heap.clear();
        other.indices.clear();
        other.next_age = 0;

        Ok(report)
    }

    fn ensure_age_space(&mut self) {
        if self.next_age == u128::MAX {
            let mut order: Vec<usize> = (0..self.heap.len()).collect();
            order.sort_by(|&a, &b| self.heap[a].age.cmp(&self.heap[b].age));
            for (age, index) in order.into_iter().enumerate() {
                self.heap[index].age = age as u128;
            }
            self.next_age = self.heap.len() as u128;
        }
    }

    fn remove_at(&mut self, index: usize) -> Entry {
        let removed = self.heap.swap_remove(index);
        self.indices.remove(&removed.item.key);
        if index < self.heap.len() {
            self.indices
                .insert(self.heap[index].item.key.clone(), index);
            self.repair(index);
        }
        removed
    }

    fn repair(&mut self, index: usize) {
        if index > 0 && compare_entries(&self.heap[index], &self.heap[(index - 1) / 2]).is_lt() {
            self.sift_up(index);
        } else {
            self.sift_down(index);
        }
    }

    fn sift_up(&mut self, mut index: usize) {
        while index > 0 {
            let parent = (index - 1) / 2;
            if !compare_entries(&self.heap[index], &self.heap[parent]).is_lt() {
                break;
            }
            self.swap_entries(index, parent);
            index = parent;
        }
    }

    fn sift_down(&mut self, mut index: usize) {
        loop {
            let left = index * 2 + 1;
            if left >= self.heap.len() {
                break;
            }
            let right = left + 1;
            let best = if right < self.heap.len()
                && compare_entries(&self.heap[right], &self.heap[left]).is_lt()
            {
                right
            } else {
                left
            };
            if !compare_entries(&self.heap[best], &self.heap[index]).is_lt() {
                break;
            }
            self.swap_entries(index, best);
            index = best;
        }
    }

    fn swap_entries(&mut self, a: usize, b: usize) {
        self.heap.swap(a, b);
        self.indices.insert(self.heap[a].item.key.clone(), a);
        self.indices.insert(self.heap[b].item.key.clone(), b);
    }
}

fn compare_entries(a: &Entry, b: &Entry) -> std::cmp::Ordering {
    a.item
        .priority
        .cmp(&b.item.priority)
        .then_with(|| a.item.value.len().cmp(&b.item.value.len()))
        .then_with(|| a.age.cmp(&b.age))
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