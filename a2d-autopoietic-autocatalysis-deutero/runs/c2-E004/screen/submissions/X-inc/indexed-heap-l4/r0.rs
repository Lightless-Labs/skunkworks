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

    for i in start..end {
        let b = bytes[i];
        let valid = if i == start {
            b.is_ascii_alphabetic()
        } else {
            b.is_ascii_alphabetic() || b.is_ascii_digit() || b == b'-'
        };
        if !valid {
            return Err(Error::InvalidKey { at: i });
        }
    }

    Ok(input[start..end].to_ascii_lowercase())
}

fn compare_entries(a: &Entry, b: &Entry) -> std::cmp::Ordering {
    a.item
        .priority
        .cmp(&b.item.priority)
        .then_with(|| a.item.value.len().cmp(&b.item.value.len()))
        .then_with(|| a.age.cmp(&b.age))
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

    fn next_age(&mut self) -> u128 {
        if self.next_age == u128::MAX {
            let mut order: Vec<usize> = (0..self.entries.len()).collect();
            order.sort_by(|&a, &b| self.entries[a].age.cmp(&self.entries[b].age));
            for (age, idx) in order.into_iter().enumerate() {
                self.entries[idx].age = age as u128;
            }
            self.next_age = self.entries.len() as u128;
        }
        let age = self.next_age;
        self.next_age += 1;
        age
    }

    fn best_index(&self) -> Option<usize> {
        self.entries
            .iter()
            .enumerate()
            .min_by(|(_, a), (_, b)| compare_entries(a, b))
            .map(|(i, _)| i)
    }

    fn worst_index(&self) -> Option<usize> {
        self.entries
            .iter()
            .enumerate()
            .max_by(|(_, a), (_, b)| compare_entries(a, b))
            .map(|(i, _)| i)
    }

    fn snapshot_item(entry: &Entry) -> Item {
        entry.item.clone()
    }

    pub fn push(
        &mut self,
        key: &str,
        priority: i64,
        value: &str,
    ) -> Result<Admission, Error> {
        let key = canonical_key(key)?;
        if self.entries.iter().any(|e| e.item.key == key) {
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
                let age = self.next_age;
                let candidate = Entry {
                    item: proposed.clone(),
                    age,
                };
                let worst = self.worst_index().expect("full nonzero queue");
                if compare_entries(&candidate, &self.entries[worst])
                    != std::cmp::Ordering::Less
                {
                    return Ok(Admission::Discarded(proposed));
                }
                let evicted = self.entries.swap_remove(worst).item;
                let age = self.next_age();
                self.entries.push(Entry {
                    item: proposed,
                    age,
                });
                return Ok(Admission::Evicted(evicted));
            }
        }

        let age = self.next_age();
        self.entries.push(Entry {
            item: proposed,
            age,
        });
        Ok(Admission::Inserted)
    }

    pub fn peek(&self) -> Option<Item> {
        self.best_index()
            .map(|i| Self::snapshot_item(&self.entries[i]))
    }

    pub fn pop(&mut self) -> Option<Item> {
        self.best_index().map(|i| self.entries.swap_remove(i).item)
    }

    pub fn get(&self, key: &str) -> Result<Option<Item>, Error> {
        let key = canonical_key(key)?;
        Ok(self
            .entries
            .iter()
            .find(|e| e.item.key == key)
            .map(|e| e.item.clone()))
    }

    fn find_index(&self, key: &str) -> Option<usize> {
        self.entries.iter().position(|e| e.item.key == key)
    }

    fn reorder_age_preserving(&mut self, index: usize) {
        let _ = index;
    }

    pub fn decrease_key(&mut self, key: &str, priority: i64) -> Result<i64, Error> {
        let key = canonical_key(key)?;
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
        let key = canonical_key(key)?;
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
        let key = canonical_key(key)?;
        let index = self
            .find_index(&key)
            .ok_or_else(|| Error::Missing { key: key.clone() })?;
        Ok(std::mem::replace(
            &mut self.entries[index].item.value,
            value.to_owned(),
        ))
    }

    pub fn remove(&mut self, key: &str) -> Result<Item, Error> {
        let key = canonical_key(key)?;
        let index = self
            .find_index(&key)
            .ok_or_else(|| Error::Missing { key: key.clone() })?;
        Ok(self.entries.swap_remove(index).item)
    }

    pub fn set_capacity(&mut self, capacity: Option<usize>) -> Vec<Item> {
        self.capacity = capacity;
        let mut evicted = Vec::new();
        if let Some(limit) = capacity {
            while self.entries.len() > limit {
                let index = self.worst_index().unwrap();
                evicted.push(self.entries.swap_remove(index).item);
            }
        }
        evicted
    }

    pub fn iter(&self) -> std::vec::IntoIter<Item> {
        let mut entries = self.entries.clone();
        entries.sort_by(compare_entries);
        entries
            .into_iter()
            .map(|e| e.item)
            .collect::<Vec<_>>()
            .into_iter()
    }

    pub fn merge(
        &mut self,
        other: &mut Self,
        policy: CollisionPolicy,
    ) -> Result<MergeReport, Error> {
        let mut source = other.entries.clone();
        source.sort_by(compare_entries);

        if policy == CollisionPolicy::Reject {
            for entry in &source {
                if self.entries.iter().any(|e| e.item.key == entry.item.key) {
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
            evicted: Vec::new(),
        };

        for entry in source {
            if let Some(index) = self.find_index(&entry.item.key) {
                match policy {
                    CollisionPolicy::KeepSelf => report.discarded += 1,
                    CollisionPolicy::TakeOther => {
                        self.entries[index].item.priority = entry.item.priority;
                        self.entries[index].item.value = entry.item.value;
                        report.replaced += 1;
                    }
                    CollisionPolicy::Reject => unreachable!(),
                }
            } else {
                let age = self.next_age();
                self.entries.push(Entry {
                    item: entry.item,
                    age,
                });
                report.moved += 1;
            }
        }

        other.entries.clear();

        if let Some(limit) = self.capacity {
            while self.entries.len() > limit {
                let index = self.worst_index().unwrap();
                report.evicted.push(self.entries.swap_remove(index).item);
            }
        }

        Ok(report)
    }
}