use std::collections::HashMap;
use std::cmp::Ordering;

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

#[derive(Clone)]
struct Entry { item: Item, age: u128 }

fn rank(a: &Entry, b: &Entry) -> Ordering {
    a.item.priority.cmp(&b.item.priority)
        .then_with(|| a.item.value.len().cmp(&b.item.value.len()))
        .then_with(|| a.age.cmp(&b.age))
}

fn canonical(raw: &str) -> Result<String, Error> {
    let b = raw.as_bytes();
    let start = b.iter().position(|&c| c != b' ').unwrap_or(b.len());
    if start == b.len() { return Err(Error::EmptyKey); }
    let end = b.iter().rposition(|&c| c != b' ').unwrap() + 1;
    for at in start..end {
        let c = b[at];
        let valid = if at == start { c.is_ascii_alphabetic() }
            else { c.is_ascii_alphanumeric() || c == b'-' };
        if !valid { return Err(Error::InvalidKey { at }); }
    }
    Ok(raw[start..end].to_ascii_lowercase())
}

pub struct IndexedHeap {
    heap: Vec<Entry>,
    index: HashMap<String, usize>,
    clock: u128,
    limit: Option<usize>,
}

impl IndexedHeap {
    pub fn new() -> Self {
        Self { heap: Vec::new(), index: HashMap::new(), clock: 0, limit: None, }
    }
    pub fn len(&self) -> usize { self.heap.len() }
    pub fn is_empty(&self) -> bool { self.heap.is_empty() }

    fn swap(&mut self, a: usize, b: usize) {
        self.heap.swap(a, b);
        self.index.insert(self.heap[a].item.key.clone(), a);
        self.index.insert(self.heap[b].item.key.clone(), b);
    }
    fn up(&mut self, mut i: usize) {
        while i > 0 {
            let p = (i - 1) / 2;
            if rank(&self.heap[i], &self.heap[p]) != Ordering::Less { break; }
            self.swap(i, p);
            i = p;
        }
    }
    fn down(&mut self, mut i: usize) {
        loop {
            let left = i * 2 + 1;
            if left >= self.len() { break; }
            let right = left + 1;
            let best = if right < self.len() && rank(&self.heap[right], &self.heap[left]) == Ordering::Less { right } else { left };
            if rank(&self.heap[best], &self.heap[i]) != Ordering::Less { break; }
            self.swap(i, best);
            i = best;
        }
    }
    fn repair(&mut self, i: usize) {
        if i > 0 && rank(&self.heap[i], &self.heap[(i - 1) / 2]) == Ordering::Less {
            self.up(i);
        } else { self.down(i); }
    }
    fn insert(&mut self, item: Item) {
        let slot = self.len();
        self.index.insert(item.key.clone(), slot);
        self.heap.push(Entry { item, age: self.clock });
        self.clock += 1;
        self.up(slot);
    }
    fn remove_at(&mut self, slot: usize) -> Item {
        let last = self.len() - 1;
        self.swap(slot, last);
        let entry = self.heap.pop().unwrap();
        self.index.remove(&entry.item.key);
        if slot < self.len() { self.repair(slot); }
        entry.item
    }
    pub fn peek(&self) -> Option<Item> { self.heap.first().map(|e| e.item.clone()) }
    pub fn pop(&mut self) -> Option<Item> {
        if self.is_empty() { None } else { Some(self.remove_at(0)) }
    }
    pub fn get(&self, key: &str) -> Result<Option<Item>, Error> {
        let key = canonical(key)?;
        Ok(self.index.get(&key).map(|&i| self.heap[i].item.clone()))
    }
    pub fn push(&mut self, key: &str, priority: i64, value: &str) -> Result<Admission, Error> {
        let key = canonical(key)?;
        if self.index.contains_key(&key) { return Err(Error::Duplicate { key }); }
        let item = Item { key, priority, value: value.to_owned() };
        if let Some(cap) = self.limit {
            if cap == 0 { return Ok(Admission::Discarded(item)); }
            if self.len() == cap {
                let worst = self.worst_index().unwrap();
                let proposed = Entry { item: item.clone(), age: self.clock };
                if rank(&proposed, &self.heap[worst]) != Ordering::Less {
                    return Ok(Admission::Discarded(item));
                }
                let evicted = self.remove_at(worst);
                self.insert(item);
                return Ok(Admission::Evicted(evicted));
            }
        }
        self.insert(item);
        Ok(Admission::Inserted)
    }
    fn locate(&self, raw: &str) -> Result<usize, Error> {
        let key = canonical(raw)?;
        self.index.get(&key).copied().ok_or(Error::Missing { key })
    }
    pub fn decrease_key(&mut self, key: &str, priority: i64) -> Result<i64, Error> {
        let i = self.locate(key)?;
        let old = self.heap[i].item.priority;
        if priority >= old { return Err(Error::WrongDirection { current: old, requested: priority }); }
        self.heap[i].item.priority = priority;
        self.repair(i);
        Ok(old)
    }
    pub fn increase_key(&mut self, key: &str, priority: i64) -> Result<i64, Error> {
        let i = self.locate(key)?;
        let old = self.heap[i].item.priority;
        if priority <= old { return Err(Error::WrongDirection { current: old, requested: priority }); }
        self.heap[i].item.priority = priority;
        self.repair(i);
        Ok(old)
    }
    pub fn replace_value(&mut self, key: &str, value: &str) -> Result<String, Error> {
        let i = self.locate(key)?;
        let old = std::mem::replace(&mut self.heap[i].item.value, value.to_owned());
        self.repair(i);
        Ok(old)
    }
    pub fn remove(&mut self, key: &str) -> Result<Item, Error> {
        let i = self.locate(key)?;
        Ok(self.remove_at(i))
    }
    fn entries_in_order(&self) -> Vec<Entry> {
        let mut entries = self.heap.clone();
        entries.sort_by(rank);
        entries
    }
    pub fn merge(&mut self, other: &mut Self, policy: CollisionPolicy) -> Result<MergeReport, Error> {
        let source = other.entries_in_order();
        if policy == CollisionPolicy::Reject {
            for entry in &source {
                if self.index.contains_key(&entry.item.key) {
                    return Err(Error::Collision { key: entry.item.key.clone() });
                }
            }
        }
        let mut report = MergeReport { moved: 0, replaced: 0, discarded: 0, evicted: Vec::new(), };
        for entry in source {
            if let Some(&i) = self.index.get(&entry.item.key) {
                match policy {
                    CollisionPolicy::KeepSelf => { report.discarded += 1; }
                    CollisionPolicy::TakeOther => {
                        self.heap[i].item = entry.item;
                        self.repair(i);
                        report.replaced += 1;
                    }
                    CollisionPolicy::Reject => unreachable!(),
                }
            } else {
                self.insert(entry.item);
                report.moved += 1;
            }
        }
        other.heap.clear();
        other.index.clear();
        report.evicted = self.trim();
        Ok(report)
    }
    pub fn with_capacity(capacity: usize) -> Self {
        let mut result = Self::new();
        result.limit = Some(capacity);
        result
    }
    pub fn capacity(&self) -> Option<usize> { self.limit }
    fn worst_index(&self) -> Option<usize> {
        (0..self.len()).max_by(|&a, &b| rank(&self.heap[a], &self.heap[b]))
    }
    fn trim(&mut self) -> Vec<Item> {
        let mut evicted = Vec::new();
        if let Some(cap) = self.limit {
            while self.len() > cap {
                let i = self.worst_index().unwrap();
                evicted.push(self.remove_at(i));
            }
        }
        evicted.reverse();
        evicted
    }
    pub fn set_capacity(&mut self, capacity: Option<usize>) -> Vec<Item> {
        self.limit = capacity;
        self.trim()
    }
    pub fn iter(&self) -> std::vec::IntoIter<Item> {
        self.entries_in_order().into_iter().map(|e| e.item).collect::<Vec<_>>().into_iter()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CollisionPolicy { KeepSelf, TakeOther, Reject }
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MergeReport {
    pub moved: usize,
    pub replaced: usize,
    pub discarded: usize,
    pub evicted: Vec<Item>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Admission { Inserted, Evicted(Item), Discarded(Item) }
