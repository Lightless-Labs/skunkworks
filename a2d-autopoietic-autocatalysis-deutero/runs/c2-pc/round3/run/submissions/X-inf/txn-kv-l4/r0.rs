use std::collections::{BTreeMap, BTreeSet, HashMap};

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
struct Value {
    value: String,
    deadline: Option<u64>,
}

#[derive(Clone)]
struct Version {
    revision: u64,
    value: Option<Value>,
}

#[derive(Clone)]
struct Write {
    value: Option<Value>,
}

struct Transaction {
    begin_revision: u64,
    read_only: bool,
    writes: BTreeMap<String, Write>,
}

pub struct Store {
    clock: u64,
    revision: u64,
    data: BTreeMap<String, Vec<Version>>,
    transactions: HashMap<u64, Transaction>,
    snapshots: BTreeMap<String, BTreeMap<String, Value>>,
}

impl Store {
    pub fn new() -> Self {
        Self {
            clock: 0,
            revision: 0,
            data: BTreeMap::new(),
            transactions: HashMap::new(),
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

    fn selected<'a>(&'a self, key: &str, tx: Option<u64>) -> Option<&'a Version> {
        let versions = self.data.get(key)?;
        match tx {
            None => versions.last(),
            Some(id) => {
                let revision = self.transactions.get(&id)?.begin_revision;
                versions.iter().rev().find(|v| v.revision <= revision)
            }
        }
    }

    fn visible_value(&self, key: &str, tx: Option<u64>, now: u64) -> Option<String> {
        if let Some(id) = tx {
            if let Some(write) = self.transactions.get(&id)?.writes.get(key) {
                return write
                    .value
                    .as_ref()
                    .filter(|v| v.deadline.map_or(true, |d| d > now))
                    .map(|v| v.value.clone());
            }
        }
        self.selected(key, tx)?
            .value
            .as_ref()
            .filter(|v| v.deadline.map_or(true, |d| d > now))
            .map(|v| v.value.clone())
    }

    fn validate_key(key: &str) -> Result<(), Error> {
        if key.is_empty() {
            Err(Error::KeyEmpty)
        } else {
            Ok(())
        }
    }

    fn append(&mut self, key: String, value: Option<Value>) {
        self.revision += 1;
        self.data.entry(key).or_default().push(Version {
            revision: self.revision,
            value,
        });
    }

    pub fn begin(&mut self, now: u64, id: u64, read_only: bool) -> Result<(), Error> {
        self.admit(now)?;
        if self.transactions.contains_key(&id) {
            return Err(Error::TransactionExists(id));
        }
        self.transactions.insert(
            id,
            Transaction {
                begin_revision: self.revision,
                read_only,
                writes: BTreeMap::new(),
            },
        );
        Ok(())
    }

    pub fn commit(&mut self, now: u64, id: u64) -> Result<u64, Error> {
        self.admit(now)?;
        let tx = self
            .transactions
            .get(&id)
            .ok_or(Error::UnknownTransaction(id))?;
        if tx.writes.is_empty() {
            self.transactions.remove(&id);
            return Ok(self.revision);
        }

        for key in tx.writes.keys() {
            if self
                .data
                .get(key)
                .and_then(|v| v.last())
                .is_some_and(|v| v.revision > tx.begin_revision)
            {
                return Err(Error::Conflict {
                    tx: id,
                    key: key.clone(),
                });
            }
        }

        let writes = self.transactions.remove(&id).unwrap().writes;
        self.revision += 1;
        let revision = self.revision;
        for (key, write) in writes {
            self.data.entry(key).or_default().push(Version {
                revision,
                value: write.value,
            });
        }
        Ok(revision)
    }

    pub fn rollback(&mut self, now: u64, id: u64) -> Result<(), Error> {
        self.admit(now)?;
        if self.transactions.remove(&id).is_none() {
            return Err(Error::UnknownTransaction(id));
        }
        Ok(())
    }

    pub fn set(
        &mut self,
        now: u64,
        tx: Option<u64>,
        key: &str,
        value: &str,
        ttl: Option<u64>,
    ) -> Result<(), Error> {
        self.admit(now)?;
        self.check_tx(tx)?;
        Self::validate_key(key)?;
        if let Some(id) = tx {
            if self.transactions.get(&id).unwrap().read_only {
                return Err(Error::ReadOnly(id));
            }
        }

        let entry = if let Some(duration) = ttl {
            let deadline = now.checked_add(duration).ok_or(Error::DeadlineOverflow)?;
            if duration == 0 {
                None
            } else {
                Some(Value {
                    value: value.to_owned(),
                    deadline: Some(deadline),
                })
            }
        } else {
            Some(Value {
                value: value.to_owned(),
                deadline: None,
            })
        };

        if let Some(id) = tx {
            self.transactions
                .get_mut(&id)
                .unwrap()
                .writes
                .insert(key.to_owned(), Write { value: entry });
        } else {
            self.append(key.to_owned(), entry);
        }
        Ok(())
    }

    pub fn delete(&mut self, now: u64, tx: Option<u64>, key: &str) -> Result<bool, Error> {
        self.admit(now)?;
        self.check_tx(tx)?;
        Self::validate_key(key)?;
        if let Some(id) = tx {
            if self.transactions.get(&id).unwrap().read_only {
                return Err(Error::ReadOnly(id));
            }
        }
        let existed = self.visible_value(key, tx, now).is_some();
        if let Some(id) = tx {
            self.transactions
                .get_mut(&id)
                .unwrap()
                .writes
                .insert(key.to_owned(), Write { value: None });
        } else {
            self.append(key.to_owned(), None);
        }
        Ok(existed)
    }

    pub fn get(
        &mut self,
        now: u64,
        tx: Option<u64>,
        key: &str,
    ) -> Result<Option<String>, Error> {
        self.admit(now)?;
        self.check_tx(tx)?;
        Self::validate_key(key)?;
        Ok(self.visible_value(key, tx, now))
    }

    fn visible_map(&self, tx: Option<u64>, now: u64) -> BTreeMap<String, String> {
        let mut keys: BTreeSet<String> = self.data.keys().cloned().collect();
        if let Some(id) = tx {
            keys.extend(self.transactions[&id].writes.keys().cloned());
        }
        let mut out = BTreeMap::new();
        for key in keys {
            if let Some(value) = self.visible_value(&key, tx, now) {
                out.insert(key, value);
            }
        }
        out
    }

    pub fn count(&mut self, now: u64, tx: Option<u64>, value: &str) -> Result<usize, Error> {
        self.admit(now)?;
        self.check_tx(tx)?;
        Ok(self
            .visible_map(tx, now)
            .values()
            .filter(|v| v.as_str() == value)
            .count())
    }

    fn in_range(key: &str, start: &str, end: Option<&str>, prefix: Option<&str>) -> bool {
        key >= start
            && end.map_or(true, |e| key < e)
            && prefix.map_or(true, |p| key.starts_with(p))
    }

    pub fn scan(
        &mut self,
        now: u64,
        tx: Option<u64>,
        start: &str,
        end: Option<&str>,
        prefix: Option<&str>,
        offset: usize,
        limit: usize,
        cursor: Option<&Cursor>,
    ) -> Result<Page, Error> {
        self.admit(now)?;
        self.check_tx(tx)?;
        if end.is_some_and(|e| e <= start) {
            return Err(Error::InvalidRange);
        }
        if let Some(c) = cursor {
            if c.start != start
                || c.end.as_deref() != end
                || c.prefix.as_deref() != prefix
                || c.offset != offset
                || c.last.is_empty()
                || !Self::in_range(&c.last, start, end, prefix)
            {
                return Err(Error::CursorMismatch);
            }
        }
        if limit == 0 {
            return Ok(Page {
                items: Vec::new(),
                next: None,
            });
        }

        let map = self.visible_map(tx, now);
        let matching: Vec<(String, String)> = map
            .into_iter()
            .filter(|(k, _)| Self::in_range(k, start, end, prefix))
            .filter(|(k, _)| cursor.map_or(true, |c| k.as_str() > c.last.as_str()))
            .collect();

        let skip = if cursor.is_some() { 0 } else { offset };
        let items: Vec<(String, String)> = matching.iter().skip(skip).take(limit).cloned().collect();
        let has_more = matching.len().saturating_sub(skip) > items.len();
        let next = if has_more {
            items.last().map(|(last, _)| Cursor {
                start: start.to_owned(),
                end: end.map(str::to_owned),
                prefix: prefix.map(str::to_owned),
                offset,
                last: last.clone(),
            })
        } else {
            None
        };
        Ok(Page { items, next })
    }

    pub fn aggregate(
        &mut self,
        now: u64,
        tx: Option<u64>,
        start: &str,
        end: Option<&str>,
        prefix: Option<&str>,
    ) -> Result<Aggregate, Error> {
        self.admit(now)?;
        self.check_tx(tx)?;
        if end.is_some_and(|e| e <= start) {
            return Err(Error::InvalidRange);
        }
        let map = self.visible_map(tx, now);
        let mut count = 0usize;
        let mut sum = 0i64;
        for (key, value) in map {
            if !Self::in_range(&key, start, end, prefix) {
                continue;
            }
            let parsed = parse_canonical_i64(&value).ok_or_else(|| Error::InvalidNumber {
                key: key.clone(),
                value: value.clone(),
            })?;
            sum = sum
                .checked_add(parsed)
                .ok_or_else(|| Error::SumOverflow { key: key.clone() })?;
            count += 1;
        }
        Ok(Aggregate { count, sum })
    }

    fn validate_name(name: &str) -> Result<(), Error> {
        if name.is_empty() {
            return Err(Error::NameEmpty);
        }
        let bytes = name.as_bytes();
        for (at, &b) in bytes.iter().enumerate() {
            let valid = if at == 0 {
                b.is_ascii_lowercase()
            } else {
                b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'_'
            };
            if !valid {
                return Err(Error::NameInvalid { at });
            }
        }
        Ok(())
    }

    pub fn snapshot(&mut self, now: u64, name: &str) -> Result<(), Error> {
        self.admit(now)?;
        Self::validate_name(name)?;
        if !self.snapshots.contains_key(name) && self.snapshots.len() >= 3 {
            return Err(Error::SnapshotLimit);
        }
        let mut saved = BTreeMap::new();
        for (key, versions) in &self.data {
            if let Some(value) = versions.last().and_then(|v| v.value.as_ref()) {
                if value.deadline.map_or(true, |d| d > now) {
                    saved.insert(key.clone(), value.clone());
                }
            }
        }
        self.snapshots.insert(name.to_owned(), saved);
        Ok(())
    }

    pub fn restore(&mut self, now: u64, name: &str) -> Result<u64, Error> {
        self.admit(now)?;
        Self::validate_name(name)?;
        let saved = self
            .snapshots
            .get(name)
            .ok_or_else(|| Error::SnapshotMissing(name.to_owned()))?;
        if !self.transactions.is_empty() {
            return Err(Error::ActiveTransactions);
        }

        let mut restored = BTreeMap::new();
        for (key, value) in saved {
            if value.deadline.map_or(true, |d| d > now) {
                restored.insert(key.clone(), value.clone());
            }
        }
        let current = self.visible_map(None, now);
        let keys: BTreeSet<String> = current.keys().chain(restored.keys()).cloned().collect();
        if keys.is_empty() {
            return Ok(self.revision);
        }

        self.revision += 1;
        let revision = self.revision;
        for key in keys {
            let value = restored.get(&key).cloned();
            self.data.entry(key).or_default().push(Version { revision, value });
        }
        Ok(revision)
    }

    pub fn forget(&mut self, now: u64, name: &str) -> Result<(), Error> {
        self.admit(now)?;
        Self::validate_name(name)?;
        if self.snapshots.remove(name).is_none() {
            return Err(Error::SnapshotMissing(name.to_owned()));
        }
        Ok(())
    }

    pub fn gc(&mut self, now: u64) -> Result<usize, Error> {
        self.admit(now)?;
        let mut removed = 0usize;
        for versions in self.data.values_mut() {
            let mut keep = BTreeSet::new();
            if !versions.is_empty() {
                keep.insert(versions.len() - 1);
            }
            for tx in self.transactions.values() {
                if let Some(index) = versions
                    .iter()
                    .rposition(|v| v.revision <= tx.begin_revision)
                {
                    keep.insert(index);
                }
            }
            let mut index = 0usize;
            versions.retain(|_| {
                let retain = keep.contains(&index);
                index += 1;
                if !retain {
                    removed += 1;
                }
                retain
            });
        }
        Ok(removed)
    }

    pub fn stats(&mut self, now: u64) -> Result<Stats, Error> {
        self.admit(now)?;
        let versions = self.data.values().map(Vec::len).sum();
        let tombstones = self
            .data
            .values()
            .flat_map(|v| v.iter())
            .filter(|v| v.value.is_none())
            .count();
        let live_keys = self.visible_map(None, now).len();
        Stats {
            clock: self.clock,
            revision: self.revision,
            keys: self.data.len(),
            live_keys,
            versions,
            tombstones,
            active_transactions: self.transactions.len(),
            read_only_transactions: self
                .transactions
                .values()
                .filter(|t| t.read_only)
                .count(),
            staged_writes: self.transactions.values().map(|t| t.writes.len()).sum(),
            snapshots: self.snapshots.len(),
        }
        .pipe(Ok)
    }
}

fn parse_canonical_i64(value: &str) -> Option<i64> {
    let bytes = value.as_bytes();
    if bytes.is_empty() {
        return None;
    }
    let digits = if bytes[0] == b'-' {
        if bytes.len() < 2 {
            return None;
        }
        &bytes[1..]
    } else {
        bytes
    };
    if digits[0] == b'0' && digits.len() > 1 {
        return None;
    }
    if !digits.iter().all(u8::is_ascii_digit) {
        return None;
    }
    if bytes[0] == b'-' && digits == b"0" {
        return None;
    }
    value.parse::<i64>().ok()
}

trait Pipe: Sized {
    fn pipe<T>(self, f: impl FnOnce(Self) -> T) -> T {
        f(self)
    }
}

impl<T> Pipe for T {}