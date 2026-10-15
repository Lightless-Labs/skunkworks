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

    pub fn push(&mut self, key: &str, priority: i64, value: &str) -> Result<(), Error> {
        let key = canonical_key(key)?;
        if self.find_index(&key).is_some() {
            return Err(Error::Duplicate { key });
        }
        self.entries.push(Entry {
            item: Item {
                key,
                priority,
                value: value.to_owned(),
            },
            age: self.next_age,
        });
        self.next_age = self.next_age.wrapping_add(1);
        Ok(())
    }

    pub fn peek(&self) -> Option<Item> {
        self.best_index().map(|i| self.entries[i].item.clone())
    }

    pub fn pop(&mut self) -> Option<Item> {
        let i = self.best_index()?;
        Some(self.entries.swap_remove(i).item)
    }

    pub fn get(&self, key: &str) -> Result<Option<Item>, Error> {
        let key = canonical_key(key)?;
        Ok(self.find_index(&key).map(|i| self.entries[i].item.clone()))
    }

    pub fn decrease_key(&mut self, key: &str, priority: i64) -> Result<i64, Error> {
        let key = canonical_key(key)?;
        let i = self.find_index(&key).ok_or_else(|| Error::Missing { key })?;
        let current = self.entries[i].item.priority;
        if priority >= current {
            return Err(Error::WrongDirection {
                current,
                requested: priority,
            });
        }
        self.entries[i].item.priority = priority;
        Ok(current)
    }

    pub fn increase_key(&mut self, key: &str, priority: i64) -> Result<i64, Error> {
        let key = canonical_key(key)?;
        let i = self.find_index(&key).ok_or_else(|| Error::Missing { key })?;
        let current = self.entries[i].item.priority;
        if priority <= current {
            return Err(Error::WrongDirection {
                current,
                requested: priority,
            });
        }
        self.entries[i].item.priority = priority;
        Ok(current)
    }

    pub fn replace_value(&mut self, key: &str, value: &str) -> Result<String, Error> {
        let key = canonical_key(key)?;
        let i = self.find_index(&key).ok_or_else(|| Error::Missing { key })?;
        Ok(std::mem::replace(
            &mut self.entries[i].item.value,
            value.to_owned(),
        ))
    }

    pub fn remove(&mut self, key: &str) -> Result<Item, Error> {
        let key = canonical_key(key)?;
        let i = self.find_index(&key).ok_or_else(|| Error::Missing { key })?;
        Ok(self.entries.swap_remove(i).item)
    }

    pub fn merge(
        &mut self,
        other: &mut Self,
        policy: CollisionPolicy,
    ) -> Result<MergeReport, Error> {
        let mut order: Vec<usize> = (0..other.entries.len()).collect();
        order.sort_by(|&a, &b| compare_entries(&other.entries[a], &other.entries[b]));

        if policy == CollisionPolicy::Reject {
            for &i in &order {
                let key = &other.entries[i].item.key;
                if self.find_index(key).is_some() {
                    return Err(Error::Collision { key: key.clone() });
                }
            }
        }

        let mut moved = 0;
        let mut replaced = 0;
        let mut discarded = 0;

        for i in order {
            let source = other.entries[i].clone();
            if let Some(receiver_index) = self.find_index(&source.item.key) {
                match policy {
                    CollisionPolicy::KeepSelf => discarded += 1,
                    CollisionPolicy::TakeOther => {
                        self.entries[receiver_index].item.priority = source.item.priority;
                        self.entries[receiver_index].item.value = source.item.value;
                        replaced += 1;
                    }
                    CollisionPolicy::Reject => unreachable!(),
                }
            } else {
                self.entries.push(Entry {
                    item: source.item,
                    age: self.next_age,
                });
                self.next_age = self.next_age.wrapping_add(1);
                moved += 1;
            }
        }

        other.entries.clear();
        other.next_age = 0;

        Ok(MergeReport {
            moved,
            replaced,
            discarded,
        })
    }

    fn find_index(&self, key: &str) -> Option<usize> {
        self.entries.iter().position(|entry| entry.item.key == key)
    }

    fn best_index(&self) -> Option<usize> {
        self.entries
            .iter()
            .enumerate()
            .min_by(|(_, a), (_, b)| compare_entries(a, b))
            .map(|(i, _)| i)
    }
}

fn compare_entries(a: &Entry, b: &Entry) -> std::cmp::Ordering {
    a.item
        .priority
        .cmp(&b.item.priority)
        .then_with(|| a.item.value.len().cmp(&b.item.value.len()))
        .then_with(|| a.age.cmp(&b.age))
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

    Ok(input[start..end].to_ascii_lowercase())
}

fn main() {}