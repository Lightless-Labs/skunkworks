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
}

#[derive(Debug, Clone)]
struct Entry {
    item: Item,
    age: u128,
}

#[derive(Debug, Clone)]
pub struct IndexedHeap {
    entries: Vec<Entry>,
    next_age: u128,
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
        self.len() == 0
    }

    fn canonicalize_key(key: &str) -> Result<String, Error> {
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

        Ok(key[start..end].to_ascii_lowercase())
    }

    fn compare_entries(a: &Entry, b: &Entry) -> std::cmp::Ordering {
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
            .min_by(|(_, a), (_, b)| Self::compare_entries(a, b))
            .map(|(i, _)| i)
    }

    fn find_index(&self, key: &str) -> Option<usize> {
        self.entries.iter().position(|entry| entry.item.key == key)
    }

    fn next_age(&mut self) {
        if self.next_age == u128::MAX {
            let mut order: Vec<usize> = (0..self.entries.len()).collect();
            order.sort_by_key(|&i| self.entries[i].age);
            for (age, index) in order.into_iter().enumerate() {
                self.entries[index].age = age as u128;
            }
            self.next_age = self.entries.len() as u128;
        }
    }

    fn rebalance_age_counter(&mut self) {
        if self.next_age == u128::MAX {
            self.next_age();
        }
    }

    pub fn push(&mut self, key: &str, priority: i64, value: &str) -> Result<(), Error> {
        let canonical = Self::canonicalize_key(key)?;
        if self.find_index(&canonical).is_some() {
            return Err(Error::Duplicate { key: canonical });
        }

        self.rebalance_age_counter();
        let age = self.next_age;
        self.next_age += 1;
        self.entries.push(Entry {
            item: Item {
                key: canonical,
                priority,
                value: value.to_owned(),
            },
            age,
        });
        Ok(())
    }

    pub fn peek(&self) -> Option<Item> {
        self.best_index()
            .map(|index| self.entries[index].item.clone())
    }

    pub fn pop(&mut self) -> Option<Item> {
        let index = self.best_index()?;
        Some(self.entries.swap_remove(index).item)
    }

    pub fn get(&self, key: &str) -> Result<Option<Item>, Error> {
        let canonical = Self::canonicalize_key(key)?;
        Ok(self
            .find_index(&canonical)
            .map(|index| self.entries[index].item.clone()))
    }

    pub fn decrease_key(&mut self, key: &str, priority: i64) -> Result<i64, Error> {
        let canonical = Self::canonicalize_key(key)?;
        let index = self
            .find_index(&canonical)
            .ok_or_else(|| Error::Missing {
                key: canonical.clone(),
            })?;
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
        let canonical = Self::canonicalize_key(key)?;
        let index = self
            .find_index(&canonical)
            .ok_or_else(|| Error::Missing {
                key: canonical.clone(),
            })?;
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
        let canonical = Self::canonicalize_key(key)?;
        let index = self
            .find_index(&canonical)
            .ok_or_else(|| Error::Missing {
                key: canonical.clone(),
            })?;
        Ok(std::mem::replace(
            &mut self.entries[index].item.value,
            value.to_owned(),
        ))
    }

    pub fn remove(&mut self, key: &str) -> Result<Item, Error> {
        let canonical = Self::canonicalize_key(key)?;
        let index = self
            .find_index(&canonical)
            .ok_or_else(|| Error::Missing {
                key: canonical.clone(),
            })?;
        Ok(self.entries.swap_remove(index).item)
    }
}

fn main() {}