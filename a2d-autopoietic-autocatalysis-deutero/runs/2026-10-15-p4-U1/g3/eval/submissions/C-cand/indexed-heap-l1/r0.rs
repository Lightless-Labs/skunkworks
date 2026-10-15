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
}

#[derive(Debug, Clone)]
struct Entry {
    item: Item,
    age: u128,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct HeapNode {
    priority: i64,
    value_len: usize,
    age: u128,
    index: usize,
}

pub struct IndexedHeap {
    entries: Vec<Entry>,
    heap: Vec<HeapNode>,
    positions: HashMap<String, usize>,
    next_age: u128,
}

impl IndexedHeap {
    pub fn new() -> Self {
        Self {
            entries: Vec::new(),
            heap: Vec::new(),
            positions: HashMap::new(),
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
        let canonical = canonicalize_key(key)?;
        if self.positions.contains_key(&canonical) {
            return Err(Error::Duplicate { key: canonical });
        }

        let age = self.next_age;
        self.next_age = self.next_age.wrapping_add(1);

        let index = self.entries.len();
        let item = Item {
            key: canonical.clone(),
            priority,
            value: value.to_owned(),
        };
        self.entries.push(Entry { item, age });
        self.positions.insert(canonical, index);

        let node = HeapNode {
            priority,
            value_len: value.len(),
            age,
            index,
        };
        self.heap.push(node);
        self.sift_up(self.heap.len() - 1);
        Ok(())
    }

    pub fn peek(&self) -> Option<Item> {
        self.heap.first().map(|node| self.entries[node.index].item.clone())
    }

    pub fn pop(&mut self) -> Option<Item> {
        if self.heap.is_empty() {
            return None;
        }

        let root = self.heap.swap_remove(0);
        if !self.heap.is_empty() {
            self.sift_down(0);
        }

        let removed = self.entries.swap_remove(root.index);
        self.positions.remove(&removed.item.key);

        if root.index < self.entries.len() {
            let moved_key = self.entries[root.index].item.key.clone();
            self.positions.insert(moved_key, root.index);
            for node in &mut self.heap {
                if node.index == self.entries.len() {
                    node.index = root.index;
                }
            }
        }

        Some(removed.item)
    }

    pub fn get(&self, key: &str) -> Result<Option<Item>, Error> {
        let canonical = canonicalize_key(key)?;
        Ok(self
            .positions
            .get(&canonical)
            .map(|&index| self.entries[index].item.clone()))
    }

    fn less(&self, a: &HeapNode, b: &HeapNode) -> bool {
        (a.priority, a.value_len, a.age) < (b.priority, b.value_len, b.age)
    }

    fn sift_up(&mut self, mut position: usize) {
        while position > 0 {
            let parent = (position - 1) / 2;
            if !self.less(&self.heap[position], &self.heap[parent]) {
                break;
            }
            self.heap.swap(position, parent);
            position = parent;
        }
    }

    fn sift_down(&mut self, mut position: usize) {
        loop {
            let left = position * 2 + 1;
            if left >= self.heap.len() {
                break;
            }
            let right = left + 1;
            let best = if right < self.heap.len()
                && self.less(&self.heap[right], &self.heap[left])
            {
                right
            } else {
                left
            };

            if !self.less(&self.heap[best], &self.heap[position]) {
                break;
            }
            self.heap.swap(position, best);
            position = best;
        }
    }
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

    let trimmed = &bytes[start..end];
    for (offset, &byte) in trimmed.iter().enumerate() {
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

fn compare_nodes(a: &HeapNode, b: &HeapNode) -> Ordering {
    (a.priority, a.value_len, a.age).cmp(&(b.priority, b.value_len, b.age))
}

fn main() {}