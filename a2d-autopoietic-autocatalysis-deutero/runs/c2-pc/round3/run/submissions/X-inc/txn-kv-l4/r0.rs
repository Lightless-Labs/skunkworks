use std::collections::{BTreeMap, BTreeSet};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Error {
    KeyEmpty,
    NameEmpty,
    NameInvalid { at: usize },
    SnapshotLimit,
    TimeBackwards { previous: u64, now: u64 },
    DeadlineOverflow,
    UnknownTransaction(u64),
    TransactionExists(u64),
    ReadOnly(u64),
    SnapshotMissing(String),
    ActiveTransactions,
    Conflict { tx: u64, key: String },
    InvalidRange,
    CursorMismatch,
    InvalidNumber { key: String, value: String },
    SumOverflow { key: String },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Stats {
    pub clock: u64,
    pub revision: u64,
    pub keys: usize,
    pub live_keys: usize,
    pub versions: usize,
    pub tombstones: usize,
    pub active_transactions: usize,
    pub read_only_transactions: usize,
    pub staged_writes: usize,
    pub snapshots: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Cursor {
    pub start: String,
    pub end: Option<String>,
    pub prefix: Option<String>,
    pub offset: usize,
    pub last: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Page {
    pub items: Vec<(String, String)>,
    pub next: Option<Cursor>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Aggregate {
    pub count: usize,
    pub sum: i64,
}

#[derive(Clone)]
struct Entry {
    value: Option<String>,
    deadline: Option<u64>,
}

#[derive(Clone)]
struct Version {
    revision: u64,
    entry: Entry,
}

struct Transaction {
    revision: u64,
    read_only: bool,
    writes: BTreeMap<String, Entry>,
}

pub struct Store {
    clock: u64,
    revision: u64,
    data: BTreeMap<String, Vec<Version>>,
    transactions: BTreeMap<u64, Transaction>,
    snapshots: BTreeMap<String, BTreeMap<String, Entry>>,
}

impl Store {
    pub fn new() -> Self {
        Self {
            clock: 0,
            revision: 0,
            data: BTreeMap::new(),
            transactions: BTreeMap::new(),
            snapshots: BTreeMap::new(),
        }
    }

    fn admit(&mut self, now: u64) -> Result<(), Error> {
        if now < self.clock {
            return Err(Error::TimeBackwards {
                previous: self.clock,
                now,
            });
        }
        self.clock = now;
        Ok(())
    }

    fn require_tx(&self, tx: u64) -> Result<(), Error> {
        if self.transactions.contains_key(&tx) {
            Ok(())
        } else {
            Err(Error::UnknownTransaction(tx))
        }
    }

    fn validate_tx(&self, tx: Option<u64>) -> Result<(), Error> {
        if let Some(id) = tx {
            self.require_tx(id)?;
        }
        Ok(())
    }

    fn visible_entry(&self, key: &str, tx: Option<u64>) -> Option<Entry> {
        if let Some(id) = tx {
            let transaction = self.transactions.get(&id)?;
            if let Some(entry) = transaction.writes.get(key) {
                return Some(entry.clone());
            }
            self.data.get(key)?.iter()
                .rev()
                .find(|v| v.revision <= transaction.revision)
                .map(|v| v.entry.clone())
        } else {
            self.data.get(key)?.last().map(|v| v.entry.clone())
        }
    }

    fn live(entry: &Entry, now: u64) -> bool {
        entry.value.is_some() && entry.deadline.map_or(true, |deadline| deadline > now)
    }

    fn visible_live(&self, key: &str, tx: Option<u64>, now: u64) -> Option<String> {
        let entry = self.visible_entry(key, tx)?;
        if Self::live(&entry, now) {
            entry.value
        } else {
            None
        }
    }

    fn append(&mut self, key: String, entry: Entry, revision: u64) {
        self.data.entry(key).or_default().push(Version { revision, entry });
    }

    fn commit_write(&mut self, key: String, entry: Entry) {
        self.revision += 1;
        self.append(key, entry, self.revision);
    }

    fn check_key(key: &str) -> Result<(), Error> {
        if key.is_empty() { Err(Error::KeyEmpty) } else { Ok(()) }
    }

    fn check_name(name: &str) -> Result<(), Error> {
        if name.is_empty() {
            return Err(Error::NameEmpty);
        }
        for (i, b) in name.bytes().enumerate() {
            let valid = if i == 0 {
                b.is_ascii_lowercase()
            } else {
                b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'_'
            };
            if !valid {
                return Err(Error::NameInvalid { at: i });
            }
        }
        Ok(())
    }

    pub fn begin(&mut self, now: u64, id: u64, read_only: bool) -> Result<(), Error> {
        self.admit(now)?;
        if self.transactions.contains_key(&id) {
            return Err(Error::TransactionExists(id));
        }
        self.transactions.insert(id, Transaction {
            revision: self.revision,
            read_only,
            writes: BTreeMap::new(),
        });
        Ok(())
    }

    pub fn commit(&mut self, now: u64, id: u64) -> Result<u64, Error> {
        self.admit(now)?;
        self.require_tx(id)?;
        let (begin_revision, writes) = {
            let tx = self.transactions.get(&id).unwrap();
            (tx.revision, tx.writes.clone())
        };
        for key in writes.keys() {
            if self.data.get(key).and_then(|v| v.last())
                .map_or(false, |v| v.revision > begin_revision)
            {
                return Err(Error::Conflict { tx: id, key: key.clone() });
            }
        }
        if writes.is_empty() {
            self.transactions.remove(&id);
            return Ok(self.revision);
        }
        self.revision += 1;
        let revision = self.revision;
        for (key, entry) in writes {
            self.append(key, entry, revision);
        }
        self.transactions.remove(&id);
        Ok(revision)
    }

    pub fn rollback(&mut self, now: u64, id: u64) -> Result<(), Error> {
        self.admit(now)?;
        self.require_tx(id)?;
        self.transactions.remove(&id);
        Ok(())
    }

    pub fn set(&mut self, now: u64, tx: Option<u64>, key: &str, value: &str, ttl: Option<u64>) -> Result<(), Error> {
        self.admit(now)?;
        self.validate_tx(tx)?;
        Self::check_key(key)?;
        if let Some(id) = tx {
            if self.transactions.get(&id).unwrap().read_only {
                return Err(Error::ReadOnly(id));
            }
        }
        let entry = match ttl {
            Some(0) => Entry { value: None, deadline: None },
            Some(d) => Entry {
                value: Some(value.to_string()),
                deadline: Some(now.checked_add(d).ok_or(Error::DeadlineOverflow)?),
            },
            None => Entry { value: Some(value.to_string()), deadline: None },
        };
        if let Some(id) = tx {
            self.transactions.get_mut(&id).unwrap().writes.insert(key.to_string(), entry);
        } else {
            self.commit_write(key.to_string(), entry);
        }
        Ok(())
    }

    pub fn delete(&mut self, now: u64, tx: Option<u64>, key: &str) -> Result<bool, Error> {
        self.admit(now)?;
        self.validate_tx(tx)?;
        Self::check_key(key)?;
        if let Some(id) = tx {
            if self.transactions.get(&id).unwrap().read_only {
                return Err(Error::ReadOnly(id));
            }
        }
        let was_live = self.visible_live(key, tx, now).is_some();
        let tombstone = Entry { value: None, deadline: None };
        if let Some(id) = tx {
            self.transactions.get_mut(&id).unwrap().writes.insert(key.to_string(), tombstone);
        } else {
            self.commit_write(key.to_string(), tombstone);
        }
        Ok(was_live)
    }

    pub fn get(&mut self, now: u64, tx: Option<u64>, key: &str) -> Result<Option<String>, Error> {
        self.admit(now)?;
        self.validate_tx(tx)?;
        Self::check_key(key)?;
        Ok(self.visible_live(key, tx, now))
    }

    fn selected_keys(&self, tx: Option<u64>, now: u64, start: &str, end: Option<&str>, prefix: Option<&str>) -> Vec<(String, String)> {
        let mut keys = BTreeSet::new();
        for key in self.data.keys() {
            keys.insert(key.clone());
        }
        if let Some(id) = tx {
            for key in self.transactions.get(&id).unwrap().writes.keys() {
                keys.insert(key.clone());
            }
        }
        keys.into_iter().filter_map(|key| {
            if key.as_str() < start || end.map_or(false, |e| key.as_str() >= e)
                || prefix.map_or(false, |p| !key.starts_with(p))
            {
                return None;
            }
            self.visible_live(&key, tx, now).map(|value| (key, value))
        }).collect()
    }

    pub fn count(&mut self, now: u64, tx: Option<u64>, value: &str) -> Result<usize, Error> {
        self.admit(now)?;
        self.validate_tx(tx)?;
        Ok(self.selected_keys(tx, now, "", None, None).iter().filter(|(_, v)| v == value).count())
    }

    pub fn scan(&mut self, now: u64, tx: Option<u64>, start: &str, end: Option<&str>, prefix: Option<&str>, offset: usize, limit: usize, cursor: Option<&Cursor>) -> Result<Page, Error> {
        self.admit(now)?;
        self.validate_tx(tx)?;
        if end.map_or(false, |e| e <= start) {
            return Err(Error::InvalidRange);
        }
        if let Some(c) = cursor {
            if c.start != start || c.end.as_deref() != end || c.prefix.as_deref() != prefix || c.offset != offset
                || c.last.is_empty() || c.last.as_str() < start
                || end.map_or(false, |e| c.last.as_str() >= e)
                || prefix.map_or(false, |p| !c.last.starts_with(p))
            {
                return Err(Error::CursorMismatch);
            }
        }
        if limit == 0 {
            return Ok(Page { items: Vec::new(), next: None });
        }
        let mut items = Vec::new();
        let mut more = false;
        let mut skipped = 0usize;
        for item in self.selected_keys(tx, now, start, end, prefix) {
            if let Some(c) = cursor {
                if item.0 <= c.last { continue; }
            } else if skipped < offset {
                skipped += 1;
                continue;
            }
            if items.len() < limit {
                items.push(item);
            } else {
                more = true;
                break;
            }
        }
        let next = if more {
            let last = items.last().unwrap().0.clone();
            Some(Cursor {
                start: start.to_string(),
                end: end.map(str::to_string),
                prefix: prefix.map(str::to_string),
                offset,
                last,
            })
        } else {
            None
        };
        Ok(Page { items, next })
    }

    pub fn aggregate(&mut self, now: u64, tx: Option<u64>, start: &str, end: Option<&str>, prefix: Option<&str>) -> Result<Aggregate, Error> {
        self.admit(now)?;
        self.validate_tx(tx)?;
        if end.map_or(false, |e| e <= start) {
            return Err(Error::InvalidRange);
        }
        let mut count = 0usize;
        let mut sum = 0i64;
        for (key, value) in self.selected_keys(tx, now, start, end, prefix) {
            let parsed = parse_canonical_i64(&value).ok_or_else(|| Error::InvalidNumber { key: key.clone(), value: value.clone() })?;
            sum = sum.checked_add(parsed).ok_or_else(|| Error::SumOverflow { key: key.clone() })?;
            count += 1;
        }
        Ok(Aggregate { count, sum })
    }

    pub fn snapshot(&mut self, now: u64, name: &str) -> Result<(), Error> {
        self.admit(now)?;
        Self::check_name(name)?;
        if !self.snapshots.contains_key(name) && self.snapshots.len() >= 3 {
            return Err(Error::SnapshotLimit);
        }
        let copy = self.selected_keys(None, now, "", None, None).into_iter()
            .filter_map(|(key, _)| {
                let entry = self.data.get(&key)?.last()?.entry.clone();
                Some((key, entry))
            }).collect();
        self.snapshots.insert(name.to_string(), copy);
        Ok(())
    }

    pub fn restore(&mut self, now: u64, name: &str) -> Result<u64, Error> {
        self.admit(now)?;
        Self::check_name(name)?;
        let saved = self.snapshots.get(name).cloned()
            .ok_or_else(|| Error::SnapshotMissing(name.to_string()))?;
        if !self.transactions.is_empty() {
            return Err(Error::ActiveTransactions);
        }
        let saved_live: BTreeMap<String, Entry> = saved.into_iter()
            .filter(|(_, entry)| Self::live(entry, now))
            .collect();
        let current_live: BTreeSet<String> = self.data.iter()
            .filter_map(|(key, versions)| versions.last().filter(|v| Self::live(&v.entry, now)).map(|_| key.clone()))
            .collect();
        let union: BTreeSet<String> = current_live.union(&saved_live.keys().cloned().collect()).cloned().collect();
        if union.is_empty() {
            return Ok(self.revision);
        }
        self.revision += 1;
        let revision = self.revision;
        for key in union {
            let entry = saved_live.get(&key).cloned()
                .unwrap_or(Entry { value: None, deadline: None });
            self.append(key, entry, revision);
        }
        Ok(revision)
    }

    pub fn forget(&mut self, now: u64, name: &str) -> Result<(), Error> {
        self.admit(now)?;
        Self::check_name(name)?;
        if self.snapshots.remove(name).is_none() {
            return Err(Error::SnapshotMissing(name.to_string()));
        }
        Ok(())
    }

    pub fn gc(&mut self, now: u64) -> Result<usize, Error> {
        self.admit(now)?;
        let mut removed = 0;
        for versions in self.data.values_mut() {
            let mut keep = BTreeSet::new();
            if let Some(last) = versions.len().checked_sub(1) {
                keep.insert(last);
            }
            for tx in self.transactions.values() {
                if let Some(index) = versions.iter().rposition(|v| v.revision <= tx.revision) {
                    keep.insert(index);
                }
            }
            let mut index = 0usize;
            versions.retain(|_| {
                let should_keep = keep.contains(&index);
                index += 1;
                if !should_keep { removed += 1; }
                should_keep
            });
        }
        Ok(removed)
    }

    pub fn stats(&mut self, now: u64) -> Result<Stats, Error> {
        self.admit(now)?;
        let versions = self.data.values().map(Vec::len).sum();
        let tombstones = self.data.values().flat_map(|v| v.iter())
            .filter(|v| v.entry.value.is_none()).count();
        let live_keys = self.data.values().filter(|v| v.last().map_or(false, |x| Self::live(&x.entry, now))).count();
        let read_only_transactions = self.transactions.values().filter(|t| t.read_only).count();
        let staged_writes = self.transactions.values().map(|t| t.writes.len()).sum();
        Ok(Stats {
            clock: self.clock,
            revision: self.revision,
            keys: self.data.len(),
            live_keys,
            versions,
            tombstones,
            active_transactions: self.transactions.len(),
            read_only_transactions,
            staged_writes,
            snapshots: self.snapshots.len(),
        })
    }
}

fn parse_canonical_i64(value: &str) -> Option<i64> {
    let bytes = value.as_bytes();
    if bytes.is_empty() {
        return None;
    }
    let digits = if bytes[0] == b'-' { &bytes[1..] } else { bytes };
    if digits.is_empty() || !digits.iter().all(u8::is_ascii_digit) {
        return None;
    }
    if digits.len() > 1 && digits[0] == b'0' {
        return None;
    }
    if bytes[0] == b'-' && digits == b"0" {
        return None;
    }
    value.parse::<i64>().ok()
}