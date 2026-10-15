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

#[derive(Debug)]
struct Entry {
    item: Item,
    age: u128,
}

#[derive(Debug)]
pub struct IndexedHeap {
    entries: Vec<Entry>,
    indices: std::collections::HashMap<String, usize>,
    next_age: u128,
}

impl IndexedHeap {
    pub fn new() -> Self {
        Self {
            entries: Vec::new(),
            indices: std::collections::HashMap::new(),
            next_age: 0,
        }
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
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

    fn better(a: &Entry, b: &Entry) -> bool {
        a.item
            .priority
            .cmp(&b.item.priority)
            .then_with(|| a.item.value.len().cmp(&b.item.value.len()))
            .then_with(|| a.age.cmp(&b.age))
            .is_lt()
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
            if !Self::better(&self.entries[index], &self.entries[parent]) {
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
            let best = if right < self.entries.len()
                && Self::better(&self.entries[right], &self.entries[left])
            {
                right
            } else {
                left
            };
            if !Self::better(&self.entries[best], &self.entries[index]) {
                break;
            }
            self.swap_slots(index, best);
            index = best;
        }
    }

    fn repair(&mut self, index: usize) {
        if index > 0 && Self::better(&self.entries[index], &self.entries[(index - 1) / 2]) {
            self.sift_up(index);
        } else {
            self.sift_down(index);
        }
    }

    fn remove_at(&mut self, index: usize) -> Item {
        let removed_key = self.entries[index].item.key.clone();
        let last = self.entries.len() - 1;
        self.entries.swap(index, last);
        let removed = self.entries.pop().expect("valid heap index");
        self.indices.remove(&removed_key);

        if index < self.entries.len() {
            let moved_key = self.entries[index].item.key.clone();
            self.indices.insert(moved_key, index);
            self.repair(index);
        }

        removed.item
    }

    pub fn push(&mut self, key: &str, priority: i64, value: &str) -> Result<(), Error> {
        let canonical = Self::canonicalize_key(key)?;
        if self.indices.contains_key(&canonical) {
            return Err(Error::Duplicate { key: canonical });
        }

        let age = self.next_age;
        self.next_age = self.next_age.saturating_add(1);

        let index = self.entries.len();
        self.entries.push(Entry {
            item: Item {
                key: canonical.clone(),
                priority,
                value: value.to_owned(),
            },
            age,
        });
        self.indices.insert(canonical, index);
        self.sift_up(index);
        Ok(())
    }

    pub fn peek(&self) -> Option<Item> {
        self.entries.first().map(|entry| entry.item.clone())
    }

    pub fn pop(&mut self) -> Option<Item> {
        if self.entries.is_empty() {
            None
        } else {
            Some(self.remove_at(0))
        }
    }

    pub fn get(&self, key: &str) -> Result<Option<Item>, Error> {
        let canonical = Self::canonicalize_key(key)?;
        Ok(self
            .indices
            .get(&canonical)
            .map(|&index| self.entries[index].item.clone()))
    }

    pub fn decrease_key(&mut self, key: &str, priority: i64) -> Result<i64, Error> {
        let canonical = Self::canonicalize_key(key)?;
        let Some(&index) = self.indices.get(&canonical) else {
            return Err(Error::Missing { key: canonical });
        };

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
        let canonical = Self::canonicalize_key(key)?;
        let Some(&index) = self.indices.get(&canonical) else {
            return Err(Error::Missing { key: canonical });
        };

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
        let canonical = Self::canonicalize_key(key)?;
        let Some(&index) = self.indices.get(&canonical) else {
            return Err(Error::Missing { key: canonical });
        };

        let previous = std::mem::replace(&mut self.entries[index].item.value, value.to_owned());
        self.repair(index);
        Ok(previous)
    }

    pub fn remove(&mut self, key: &str) -> Result<Item, Error> {
        let canonical = Self::canonicalize_key(key)?;
        let Some(&index) = self.indices.get(&canonical) else {
            return Err(Error::Missing { key: canonical });
        };
        Ok(self.remove_at(index))
    }
}

fn main() {}