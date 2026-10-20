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
}

struct Entry {
    item: Item,
    age: u64,
}

pub struct IndexedHeap {
    slots: Vec<Entry>,
    positions: HashMap<String, usize>,
    age_counter: u64,
}

impl IndexedHeap {
    pub fn new() -> Self {
        Self {
            slots: Vec::new(),
            positions: HashMap::new(),
            age_counter: 0,
        }
    }

    pub fn len(&self) -> usize {
        self.slots.len()
    }

    pub fn is_empty(&self) -> bool {
        self.slots.is_empty()
    }

    fn canonicalize(input: &str) -> Result<String, Error> {
        let bytes = input.as_bytes();
        let mut first = 0usize;
        let mut last = bytes.len();

        while first < last && bytes[first] == 0x20 {
            first += 1;
        }
        while first < last && bytes[last - 1] == 0x20 {
            last -= 1;
        }

        if first == last {
            return Err(Error::EmptyKey);
        }

        for pos in first..last {
            let byte = bytes[pos];
            let allowed = if pos == first {
                byte.is_ascii_alphabetic()
            } else {
                byte.is_ascii_alphabetic() || byte.is_ascii_digit() || byte == b'-'
            };
            if !allowed {
                return Err(Error::InvalidKey { at: pos });
            }
        }

        Ok(input[first..last].to_ascii_lowercase())
    }

    fn comes_before(left: &Entry, right: &Entry) -> bool {
        match left.item.priority.cmp(&right.item.priority) {
            std::cmp::Ordering::Less => true,
            std::cmp::Ordering::Greater => false,
            std::cmp::Ordering::Equal => match left.item.value.len().cmp(&right.item.value.len()) {
                std::cmp::Ordering::Less => true,
                std::cmp::Ordering::Greater => false,
                std::cmp::Ordering::Equal => left.age < right.age,
            },
        }
    }

    fn swap_entries(&mut self, i: usize, j: usize) {
        self.slots.swap(i, j);
        let ki = self.slots[i].item.key.clone();
        let kj = self.slots[j].item.key.clone();
        self.positions.insert(ki, i);
        self.positions.insert(kj, j);
    }

    fn bubble_up(&mut self, mut i: usize) {
        while i != 0 {
            let p = (i - 1) / 2;
            if !Self::comes_before(&self.slots[i], &self.slots[p]) {
                return;
            }
            self.swap_entries(i, p);
            i = p;
        }
    }

    fn bubble_down(&mut self, mut i: usize) {
        loop {
            let left = i.saturating_mul(2).saturating_add(1);
            if left >= self.slots.len() {
                return;
            }
            let right = left + 1;
            let child = if right < self.slots.len()
                && Self::comes_before(&self.slots[right], &self.slots[left])
            {
                right
            } else {
                left
            };
            if !Self::comes_before(&self.slots[child], &self.slots[i]) {
                return;
            }
            self.swap_entries(i, child);
            i = child;
        }
    }

    fn rebalance(&mut self, index: usize) {
        if index > 0 && Self::comes_before(&self.slots[index], &self.slots[(index - 1) / 2]) {
            self.bubble_up(index);
        } else {
            self.bubble_down(index);
        }
    }

    fn detach(&mut self, index: usize) -> Entry {
        let last = self.slots.len() - 1;
        self.slots.swap(index, last);
        let removed = self.slots.pop().unwrap();
        self.positions.remove(&removed.item.key);
        if index < self.slots.len() {
            self.positions
                .insert(self.slots[index].item.key.clone(), index);
            self.rebalance(index);
        }
        removed
    }

    pub fn push(&mut self, key: &str, priority: i64, value: &str) -> Result<(), Error> {
        let key = Self::canonicalize(key)?;
        if self.positions.contains_key(&key) {
            return Err(Error::Duplicate { key });
        }
        let entry = Entry {
            item: Item {
                key: key.clone(),
                priority,
                value: value.to_owned(),
            },
            age: self.age_counter,
        };
        self.age_counter = self.age_counter.wrapping_add(1);
        let index = self.slots.len();
        self.slots.push(entry);
        self.positions.insert(key, index);
        self.bubble_up(index);
        Ok(())
    }

    pub fn peek(&self) -> Option<Item> {
        self.slots.get(0).map(|entry| entry.item.clone())
    }

    pub fn pop(&mut self) -> Option<Item> {
        if self.slots.is_empty() {
            None
        } else {
            Some(self.detach(0).item)
        }
    }

    pub fn get(&self, key: &str) -> Result<Option<Item>, Error> {
        let key = Self::canonicalize(key)?;
        Ok(self.positions.get(&key).map(|&i| self.slots[i].item.clone()))
    }

    pub fn decrease_key(&mut self, key: &str, priority: i64) -> Result<i64, Error> {
        let key = Self::canonicalize(key)?;
        let index = match self.positions.get(&key) {
            Some(&index) => index,
            None => return Err(Error::Missing { key }),
        };
        let current = self.slots[index].item.priority;
        if priority >= current {
            return Err(Error::WrongDirection {
                current,
                requested: priority,
            });
        }
        self.slots[index].item.priority = priority;
        self.rebalance(index);
        Ok(current)
    }

    pub fn increase_key(&mut self, key: &str, priority: i64) -> Result<i64, Error> {
        let key = Self::canonicalize(key)?;
        let index = match self.positions.get(&key) {
            Some(&index) => index,
            None => return Err(Error::Missing { key }),
        };
        let current = self.slots[index].item.priority;
        if priority <= current {
            return Err(Error::WrongDirection {
                current,
                requested: priority,
            });
        }
        self.slots[index].item.priority = priority;
        self.rebalance(index);
        Ok(current)
    }

    pub fn replace_value(&mut self, key: &str, value: &str) -> Result<String, Error> {
        let key = Self::canonicalize(key)?;
        let index = match self.positions.get(&key) {
            Some(&index) => index,
            None => return Err(Error::Missing { key }),
        };
        let old = std::mem::replace(&mut self.slots[index].item.value, value.to_owned());
        self.rebalance(index);
        Ok(old)
    }

    pub fn remove(&mut self, key: &str) -> Result<Item, Error> {
        let key = Self::canonicalize(key)?;
        let index = match self.positions.get(&key) {
            Some(&index) => index,
            None => return Err(Error::Missing { key }),
        };
        Ok(self.detach(index).item)
    }
}
