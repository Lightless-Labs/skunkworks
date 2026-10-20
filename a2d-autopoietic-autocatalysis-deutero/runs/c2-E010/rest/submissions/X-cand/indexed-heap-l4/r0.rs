use std::collections::BTreeMap;

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
    entries: BTreeMap<String, Entry>,
    capacity: Option<usize>,
    next_age: u128,
}

impl IndexedHeap {
    pub fn new() -> Self {
        Self::create(None)
    }

    pub fn with_capacity(capacity: usize) -> Self {
        Self::create(Some(capacity))
    }

    fn create(capacity: Option<usize>) -> Self {
        Self {
            entries: BTreeMap::new(),
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

    pub fn push(
        &mut self,
        key: &str,
        priority: i64,
        value: &str,
    ) -> Result<Admission, Error> {
        let key = normalize(key)?;
        if self.entries.contains_key(&key) {
            return Err(Error::Duplicate { key });
        }

        let item = Item {
            key: key.clone(),
            priority,
            value: value.to_owned(),
        };
        if self.capacity == Some(0) {
            return Ok(Admission::Discarded(item));
        }

        if let Some(limit) = self.capacity {
            if self.len() >= limit {
                let worst = self.worst_key().unwrap();
                let proposed = Entry {
                    item: item.clone(),
                    age: self.next_age,
                };
                if compare(&proposed, self.entries.get(&worst).unwrap()) != std::cmp::Ordering::Less {
                    return Ok(Admission::Discarded(item));
                }
                let evicted = self.entries.remove(&worst).unwrap().item;
                self.insert_fresh(key, item);
                return Ok(Admission::Evicted(evicted));
            }
        }

        self.insert_fresh(key, item);
        Ok(Admission::Inserted)
    }

    pub fn peek(&self) -> Option<Item> {
        self.best_key().and_then(|key| self.entries.get(&key).map(|e| e.item.clone()))
    }

    pub fn pop(&mut self) -> Option<Item> {
        let key = self.best_key()?;
        self.entries.remove(&key).map(|e| e.item)
    }

    pub fn get(&self, key: &str) -> Result<Option<Item>, Error> {
        let key = normalize(key)?;
        Ok(self.entries.get(&key).map(|e| e.item.clone()))
    }

    pub fn decrease_key(&mut self, key: &str, priority: i64) -> Result<i64, Error> {
        let key = normalize(key)?;
        let current = self
            .entries
            .get(&key)
            .ok_or_else(|| Error::Missing { key: key.clone() })?
            .item
            .priority;
        if priority >= current {
            return Err(Error::WrongDirection { current, requested: priority });
        }
        self.entries.get_mut(&key).unwrap().item.priority = priority;
        Ok(current)
    }

    pub fn increase_key(&mut self, key: &str, priority: i64) -> Result<i64, Error> {
        let key = normalize(key)?;
        let current = self
            .entries
            .get(&key)
            .ok_or_else(|| Error::Missing { key: key.clone() })?
            .item
            .priority;
        if priority <= current {
            return Err(Error::WrongDirection { current, requested: priority });
        }
        self.entries.get_mut(&key).unwrap().item.priority = priority;
        Ok(current)
    }

    pub fn replace_value(&mut self, key: &str, value: &str) -> Result<String, Error> {
        let key = normalize(key)?;
        let entry = self
            .entries
            .get_mut(&key)
            .ok_or_else(|| Error::Missing { key: key.clone() })?;
        Ok(std::mem::replace(&mut entry.item.value, value.to_owned()))
    }

    pub fn remove(&mut self, key: &str) -> Result<Item, Error> {
        let key = normalize(key)?;
        self.entries
            .remove(&key)
            .map(|entry| entry.item)
            .ok_or(Error::Missing { key })
    }

    pub fn set_capacity(&mut self, capacity: Option<usize>) -> Vec<Item> {
        self.capacity = capacity;
        let count = capacity.map_or(0, |limit| self.len().saturating_sub(limit));
        (0..count)
            .map(|_| {
                let key = self.worst_key().unwrap();
                self.entries.remove(&key).unwrap().item
            })
            .collect()
    }

    pub fn iter(&self) -> std::vec::IntoIter<Item> {
        self.ordered_keys()
            .into_iter()
            .map(|key| self.entries.get(&key).unwrap().item.clone())
            .collect::<Vec<_>>()
            .into_iter()
    }

    pub fn merge(
        &mut self,
        other: &mut Self,
        policy: CollisionPolicy,
    ) -> Result<MergeReport, Error> {
        let source_order = other.ordered_keys();
        if policy == CollisionPolicy::Reject {
            for key in &source_order {
                if self.entries.contains_key(key) {
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

        for key in source_order {
            let entry = other.entries.remove(&key).unwrap();
            if let Some(existing) = self.entries.get_mut(&key) {
                match policy {
                    CollisionPolicy::KeepSelf => report.discarded += 1,
                    CollisionPolicy::TakeOther => {
                        existing.item.priority = entry.item.priority;
                        existing.item.value = entry.item.value;
                        report.replaced += 1;
                    }
                    CollisionPolicy::Reject => unreachable!(),
                }
            } else {
                self.insert_fresh(key, entry.item);
                report.moved += 1;
            }
        }

        let count = self.capacity.map_or(0, |limit| self.len().saturating_sub(limit));
        for _ in 0..count {
            let key = self.worst_key().unwrap();
            report.evicted.push(self.entries.remove(&key).unwrap().item);
        }

        Ok(report)
    }

    fn insert_fresh(&mut self, key: String, item: Item) {
        if self.next_age == u128::MAX {
            self.renumber();
        }
        let age = self.next_age;
        self.next_age += 1;
        self.entries.insert(key, Entry { item, age });
    }

    fn best_key(&self) -> Option<String> {
        self.entries
            .values()
            .min_by(|a, b| compare(a, b))
            .map(|entry| entry.item.key.clone())
    }

    fn worst_key(&self) -> Option<String> {
        self.entries
            .values()
            .max_by(|a, b| compare(a, b))
            .map(|entry| entry.item.key.clone())
    }

    fn ordered_keys(&self) -> Vec<String> {
        let mut entries: Vec<&Entry> = self.entries.values().collect();
        entries.sort_by(|a, b| compare(a, b));
        entries.iter().map(|entry| entry.item.key.clone()).collect()
    }

    fn renumber(&mut self) {
        let mut ages: Vec<(String, u128)> = self
            .entries
            .values()
            .map(|entry| (entry.item.key.clone(), entry.age))
            .collect();
        ages.sort_by_key(|(_, age)| *age);
        for (index, (key, _)) in ages.into_iter().enumerate() {
            self.entries.get_mut(&key).unwrap().age = index as u128;
        }
        self.next_age = self.len() as u128;
    }
}

fn compare(a: &Entry, b: &Entry) -> std::cmp::Ordering {
    a.item
        .priority
        .cmp(&b.item.priority)
        .then_with(|| a.item.value.len().cmp(&b.item.value.len()))
        .then_with(|| a.age.cmp(&b.age))
}

fn normalize(input: &str) -> Result<String, Error> {
    let bytes = input.as_bytes();
    let mut begin = 0;
    let mut end = bytes.len();
    while begin < end && bytes[begin] == b' ' {
        begin += 1;
    }
    while begin < end && bytes[end - 1] == b' ' {
        end -= 1;
    }
    if begin == end {
        return Err(Error::EmptyKey);
    }

    for (i, byte) in bytes[begin..end].iter().copied().enumerate() {
        let valid = if i == 0 {
            byte.is_ascii_alphabetic()
        } else {
            byte.is_ascii_alphanumeric() || byte == b'-'
        };
        if !valid {
            return Err(Error::InvalidKey { at: begin + i });
        }
    }
    Ok(input[begin..end].to_ascii_lowercase())
}
