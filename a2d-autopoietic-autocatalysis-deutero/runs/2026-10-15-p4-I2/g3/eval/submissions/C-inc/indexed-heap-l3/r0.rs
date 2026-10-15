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
    age: u64,
}

pub struct IndexedHeap {
    entries: Vec<Entry>,
    next_age: u64,
}

impl IndexedHeap {
    pub fn new() -> Self {
        Self {
            entries: Vec::new(),
            next_age: 0,
        }
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
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

    fn before(a: &Entry, b: &Entry) -> bool {
        a.item
            .priority
            .cmp(&b.item.priority)
            .then_with(|| a.item.value.len().cmp(&b.item.value.len()))
            .then_with(|| a.age.cmp(&b.age))
            .is_lt()
    }

    fn best_index(&self) -> Option<usize> {
        (0..self.entries.len()).min_by(|&a, &b| {
            if Self::before(&self.entries[a], &self.entries[b]) {
                std::cmp::Ordering::Less
            } else if Self::before(&self.entries[b], &self.entries[a]) {
                std::cmp::Ordering::Greater
            } else {
                std::cmp::Ordering::Equal
            }
        })
    }

    fn index_of(&self, key: &str) -> Option<usize> {
        self.entries.iter().position(|entry| entry.item.key == key)
    }

    fn fresh_age(&mut self) -> u64 {
        if self.next_age == u64::MAX {
            let mut order: Vec<usize> = (0..self.entries.len()).collect();
            order.sort_by_key(|&i| self.entries[i].age);
            for (age, index) in order.into_iter().enumerate() {
                self.entries[index].age = age as u64;
            }
            self.next_age = self.entries.len() as u64;
        }
        let age = self.next_age;
        self.next_age += 1;
        age
    }

    pub fn push(&mut self, key: &str, priority: i64, value: &str) -> Result<(), Error> {
        let key = Self::canonical_key(key)?;
        if self.index_of(&key).is_some() {
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
        Ok(())
    }

    pub fn peek(&self) -> Option<Item> {
        self.best_index().map(|index| self.entries[index].item.clone())
    }

    pub fn pop(&mut self) -> Option<Item> {
        let index = self.best_index()?;
        Some(self.entries.swap_remove(index).item)
    }

    pub fn get(&self, key: &str) -> Result<Option<Item>, Error> {
        let key = Self::canonical_key(key)?;
        Ok(self
            .index_of(&key)
            .map(|index| self.entries[index].item.clone()))
    }

    pub fn decrease_key(&mut self, key: &str, priority: i64) -> Result<i64, Error> {
        let key = Self::canonical_key(key)?;
        let index = self
            .index_of(&key)
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
            .index_of(&key)
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
            .index_of(&key)
            .ok_or_else(|| Error::Missing { key: key.clone() })?;
        let previous =
            std::mem::replace(&mut self.entries[index].item.value, value.to_owned());
        Ok(previous)
    }

    pub fn remove(&mut self, key: &str) -> Result<Item, Error> {
        let key = Self::canonical_key(key)?;
        let index = self
            .index_of(&key)
            .ok_or_else(|| Error::Missing { key: key.clone() })?;
        Ok(self.entries.swap_remove(index).item)
    }

    pub fn merge(
        &mut self,
        other: &mut Self,
        policy: CollisionPolicy,
    ) -> Result<MergeReport, Error> {
        let source_len = other.len();
        let mut order = Vec::with_capacity(source_len);
        while let Some(index) = other.best_index() {
            order.push(other.entries.swap_remove(index));
        }

        if policy == CollisionPolicy::Reject {
            let collision_key = order
                .iter()
                .find(|entry| self.index_of(&entry.item.key).is_some())
                .map(|entry| entry.item.key.clone());

            if let Some(key) = collision_key {
                other.entries = order;
                return Err(Error::Collision { key });
            }
        }

        let mut report = MergeReport {
            moved: 0,
            replaced: 0,
            discarded: 0,
        };

        for source in order {
            if let Some(index) = self.index_of(&source.item.key) {
                match policy {
                    CollisionPolicy::KeepSelf => report.discarded += 1,
                    CollisionPolicy::TakeOther => {
                        self.entries[index].item.priority = source.item.priority;
                        self.entries[index].item.value = source.item.value;
                        report.replaced += 1;
                    }
                    CollisionPolicy::Reject => unreachable!(),
                }
            } else {
                let age = self.fresh_age();
                self.entries.push(Entry {
                    item: source.item,
                    age,
                });
                report.moved += 1;
            }
        }

        Ok(report)
    }
}

fn main() {}