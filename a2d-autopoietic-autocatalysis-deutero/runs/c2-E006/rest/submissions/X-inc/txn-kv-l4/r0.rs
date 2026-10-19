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

pub struct Store {
    clock: u64,
    revision: u64,
    data: BTreeMap<String, Vec<Version>>,
    transactions: BTreeMap<u64, Transaction>,
    snapshots: BTreeMap<String, BTreeMap<String, Entry>>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct Entry {
    value: String,
    deadline: Option<u64>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum VersionValue {
    Value(Entry),
    Tombstone,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct Version {
    revision: u64,
    value: VersionValue,
}

#[derive(Debug, Clone)]
struct Transaction {
    begin_revision: u64,
    read_only: bool,
    writes: BTreeMap<String, VersionValue>,
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

    fn check_tx(&self, tx: Option<u64>) -> Result<(), Error> {
        if let Some(id) = tx {
            if !self.transactions.contains_key(&id) {
                return Err(Error::UnknownTransaction(id));
            }
        }
        Ok(())
    }

    fn check_key(key: &str) -> Result<(), Error> {
        if key.is_empty() {
            Err(Error::KeyEmpty)
        } else {
            Ok(())
        }
    }

    fn selected<'a>(&'a self, key: &str, tx: Option<u64>) -> Option<&'a VersionValue> {
        if let Some(id) = tx {
            let t = self.transactions.get(&id)?;
            if let Some(v) = t.writes.get(key) {
                return Some(v);
            }
            self.data.get(key)?.iter()
                .rev()
                .find(|v| v.revision <= t.begin_revision)
                .map(|v| &v.value)
        } else {
            self.data.get(key)?.last().map(|v| &v.value)
        }
    }

    fn live_value<'a>(&self, value: &'a VersionValue, now: u64) -> Option<&'a str> {
        match value {
            VersionValue::Value(e) if e.deadline.map_or(true, |d| d > now) => Some(&e.value),
            _ => None,
        }
    }

    fn view(&self, tx: Option<u64>, now: u64) -> BTreeMap<String, String> {
        let keys: BTreeSet<String> = self.data.keys().cloned()
            .chain(tx.and_then(|id| self.transactions.get(&id).map(|t| t.writes.keys().cloned().collect::<Vec<_>>())).into_iter().flatten())
            .collect();
        let mut out = BTreeMap::new();
        for key in keys {
            if let Some(v) = self.selected(&key, tx) {
                if let Some(value) = self.live_value(v, now) {
                    out.insert(key, value.to_string());
                }
            }
        }
        out
    }

    fn append(&mut self, key: String, value: VersionValue, revision: u64) {
        self.data.entry(key).or_default().push(Version { revision, value });
    }

    pub fn begin(&mut self, now: u64, id: u64, read_only: bool) -> Result<(), Error> {
        self.admit(now)?;
        if self.transactions.contains_key(&id) {
            return Err(Error::TransactionExists(id));
        }
        self.transactions.insert(id, Transaction {
            begin_revision: self.revision,
            read_only,
            writes: BTreeMap::new(),
        });
        Ok(())
    }

    pub fn commit(&mut self, now: u64, id: u64) -> Result<u64, Error> {
        self.admit(now)?;
        let tx = self.transactions.get(&id).ok_or(Error::UnknownTransaction(id))?;
        if !tx.writes.is_empty() {
            for key in tx.writes.keys() {
                if self.data.get(key).and_then(|v| v.last())
                    .map_or(false, |v| v.revision > tx.begin_revision)
                {
                    return Err(Error::Conflict { tx: id, key: key.clone() });
                }
            }
            let next = self.revision + 1;
            let writes = tx.writes.clone();
            for (key, value) in writes {
                self.append(key, value, next);
            }
            self.revision = next;
        }
        self.transactions.remove(&id);
        Ok(self.revision)
    }

    pub fn rollback(&mut self, now: u64, id: u64) -> Result<(), Error> {
        self.admit(now)?;
        if self.transactions.remove(&id).is_none() {
            return Err(Error::UnknownTransaction(id));
        }
        Ok(())
    }

    pub fn set(&mut self, now: u64, tx: Option<u64>, key: &str, value: &str, ttl: Option<u64>) -> Result<(), Error> {
        self.admit(now)?;
        self.check_tx(tx)?;
        Self::check_key(key)?;
        if let Some(id) = tx {
            if self.transactions.get(&id).unwrap().read_only {
                return Err(Error::ReadOnly(id));
            }
        }
        let val = match ttl {
            Some(0) => VersionValue::Tombstone,
            Some(d) => VersionValue::Value(Entry {
                value: value.to_string(),
                deadline: Some(now.checked_add(d).ok_or(Error::DeadlineOverflow)?),
            }),
            None => VersionValue::Value(Entry { value: value.to_string(), deadline: None }),
        };
        if let Some(id) = tx {
            self.transactions.get_mut(&id).unwrap().writes.insert(key.to_string(), val);
        } else {
            self.revision += 1;
            self.append(key.to_string(), val, self.revision);
        }
        Ok(())
    }

    pub fn delete(&mut self, now: u64, tx: Option<u64>, key: &str) -> Result<bool, Error> {
        self.admit(now)?;
        self.check_tx(tx)?;
        Self::check_key(key)?;
        if let Some(id) = tx {
            if self.transactions.get(&id).unwrap().read_only {
                return Err(Error::ReadOnly(id));
            }
        }
        let existed = self.selected(key, tx).and_then(|v| self.live_value(v, now)).is_some();
        if let Some(id) = tx {
            self.transactions.get_mut(&id).unwrap().writes.insert(key.to_string(), VersionValue::Tombstone);
        } else {
            self.revision += 1;
            self.append(key.to_string(), VersionValue::Tombstone, self.revision);
        }
        Ok(existed)
    }

    pub fn get(&mut self, now: u64, tx: Option<u64>, key: &str) -> Result<Option<String>, Error> {
        self.admit(now)?;
        self.check_tx(tx)?;
        Self::check_key(key)?;
        Ok(self.selected(key, tx).and_then(|v| self.live_value(v, now)).map(str::to_string))
    }

    pub fn count(&mut self, now: u64, tx: Option<u64>, value: &str) -> Result<usize, Error> {
        self.admit(now)?;
        self.check_tx(tx)?;
        Ok(self.view(tx, now).values().filter(|v| v.as_str() == value).count())
    }

    fn check_range(start: &str, end: Option<&str>) -> Result<(), Error> {
        if end.map_or(false, |e| e <= start) {
            Err(Error::InvalidRange)
        } else {
            Ok(())
        }
    }

    fn matches(key: &str, start: &str, end: Option<&str>, prefix: Option<&str>) -> bool {
        key >= start
            && end.map_or(true, |e| key < e)
            && prefix.map_or(true, |p| key.starts_with(p))
    }

    pub fn scan(&mut self, now: u64, tx: Option<u64>, start: &str, end: Option<&str>, prefix: Option<&str>, offset: usize, limit: usize, cursor: Option<&Cursor>) -> Result<Page, Error> {
        self.admit(now)?;
        self.check_tx(tx)?;
        Self::check_range(start, end)?;
        if let Some(c) = cursor {
            if c.start != start || c.end.as_deref() != end || c.prefix.as_deref() != prefix || c.offset != offset
                || c.last.is_empty() || !Self::matches(&c.last, start, end, prefix)
            {
                return Err(Error::CursorMismatch);
            }
        }
        if limit == 0 {
            return Ok(Page { items: Vec::new(), next: None });
        }
        let view = self.view(tx, now);
        let mut keys: Vec<String> = view.keys()
            .filter(|k| Self::matches(k, start, end, prefix))
            .filter(|k| cursor.map_or(true, |c| k.as_str() > c.last.as_str()))
            .cloned().collect();
        if cursor.is_none() {
            keys = keys.into_iter().skip(offset).collect();
        }
        let has_more = keys.len() > limit;
        let items: Vec<(String, String)> = keys.into_iter().take(limit)
            .map(|k| (k.clone(), view[&k].clone())).collect();
        let next = if has_more {
            items.last().map(|(last, _)| Cursor {
                start: start.to_string(),
                end: end.map(str::to_string),
                prefix: prefix.map(str::to_string),
                offset,
                last: last.clone(),
            })
        } else {
            None
        };
        Ok(Page { items, next })
    }

    pub fn aggregate(&mut self, now: u64, tx: Option<u64>, start: &str, end: Option<&str>, prefix: Option<&str>) -> Result<Aggregate, Error> {
        self.admit(now)?;
        self.check_tx(tx)?;
        Self::check_range(start, end)?;
        let view = self.view(tx, now);
        let mut count = 0usize;
        let mut sum = 0i64;
        for (key, value) in view {
            if !Self::matches(&key, start, end, prefix) {
                continue;
            }
            let parsed = parse_number(&value).ok_or_else(|| Error::InvalidNumber { key: key.clone(), value: value.clone() })?;
            sum = sum.checked_add(parsed).ok_or_else(|| Error::SumOverflow { key: key.clone() })?;
            count += 1;
        }
        Ok(Aggregate { count, sum })
    }

    pub fn snapshot(&mut self, now: u64, name: &str) -> Result<(), Error> {
        self.admit(now)?;
        Self::check_name(name)?;
        let saved: BTreeMap<String, Entry> = self.view(None, now).into_iter().filter_map(|(k, value)| {
            let entry = self.data.get(&k)?.last()?;
            match &entry.value {
                VersionValue::Value(e) => Some((k, e.clone())),
                VersionValue::Tombstone => { let _ = value; None }
            }
        }).collect();
        if !self.snapshots.contains_key(name) && self.snapshots.len() >= 3 {
            return Err(Error::SnapshotLimit);
        }
        self.snapshots.insert(name.to_string(), saved);
        Ok(())
    }

    pub fn restore(&mut self, now: u64, name: &str) -> Result<u64, Error> {
        self.admit(now)?;
        Self::check_name(name)?;
        let saved = self.snapshots.get(name).cloned().ok_or_else(|| Error::SnapshotMissing(name.to_string()))?;
        if !self.transactions.is_empty() {
            return Err(Error::ActiveTransactions);
        }
        let saved_live: BTreeMap<String, Entry> = saved.into_iter()
            .filter(|(_, e)| e.deadline.map_or(true, |d| d > now)).collect();
        let current = self.view(None, now);
        let keys: BTreeSet<String> = current.keys().cloned().chain(saved_live.keys().cloned()).collect();
        if keys.is_empty() {
            return Ok(self.revision);
        }
        self.revision += 1;
        let rev = self.revision;
        for key in keys {
            let v = saved_live.get(&key).cloned().map(VersionValue::Value).unwrap_or(VersionValue::Tombstone);
            self.append(key, v, rev);
        }
        Ok(rev)
    }

    pub fn forget(&mut self, now: u64, name: &str) -> Result<(), Error> {
        self.admit(now)?;
        Self::check_name(name)?;
        self.snapshots.remove(name).ok_or_else(|| Error::SnapshotMissing(name.to_string()))?;
        Ok(())
    }

    fn check_name(name: &str) -> Result<(), Error> {
        if name.is_empty() {
            return Err(Error::NameEmpty);
        }
        for (i, b) in name.bytes().enumerate() {
            let ok = if i == 0 { b.is_ascii_lowercase() } else { b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'_' };
            if !ok {
                return Err(Error::NameInvalid { at: i });
            }
        }
        Ok(())
    }

    pub fn gc(&mut self, now: u64) -> Result<usize, Error> {
        self.admit(now)?;
        let mut removed = 0;
        for versions in self.data.values_mut() {
            if versions.len() <= 1 { continue; }
            let mut keep = BTreeSet::new();
            keep.insert(versions.len() - 1);
            for tx in self.transactions.values() {
                if let Some(i) = versions.iter().rposition(|v| v.revision <= tx.begin_revision) {
                    keep.insert(i);
                }
            }
            let mut index = 0usize;
            versions.retain(|_| {
                let retain = keep.contains(&index);
                index += 1;
                if !retain { removed += 1; }
                retain
            });
        }
        Ok(removed)
    }

    pub fn stats(&mut self, now: u64) -> Result<Stats, Error> {
        self.admit(now)?;
        let versions = self.data.values().map(Vec::len).sum();
        let tombstones = self.data.values().flat_map(|v| v.iter())
            .filter(|v| matches!(v.value, VersionValue::Tombstone)).count();
        let live_keys = self.view(None, now).len();
        Ok(Stats {
            clock: self.clock,
            revision: self.revision,
            keys: self.data.len(),
            live_keys,
            versions,
            tombstones,
            active_transactions: self.transactions.len(),
            read_only_transactions: self.transactions.values().filter(|t| t.read_only).count(),
            staged_writes: self.transactions.values().map(|t| t.writes.len()).sum(),
            snapshots: self.snapshots.len(),
        })
    }
}

fn parse_number(s: &str) -> Option<i64> {
    if s.is_empty() || s == "-0" { return None; }
    let digits = s.strip_prefix('-').unwrap_or(s);
    if digits.is_empty() || !digits.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    if digits.len() > 1 && digits.starts_with('0') {
        return None;
    }
    if s.starts_with('+') { return None; }
    s.parse::<i64>().ok()
}