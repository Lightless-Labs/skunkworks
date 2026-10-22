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
struct Version {
    revision: u64,
    value: Option<String>,
    deadline: Option<u64>,
}

#[derive(Clone)]
struct Write {
    value: Option<String>,
    deadline: Option<u64>,
}

struct Transaction {
    begin_revision: u64,
    read_only: bool,
    writes: BTreeMap<String, Write>,
}

#[derive(Clone)]
struct Snapshot {
    entries: BTreeMap<String, (String, Option<u64>)>,
}

pub struct Store {
    clock: u64,
    revision: u64,
    versions: BTreeMap<String, Vec<Version>>,
    transactions: HashMap<u64, Transaction>,
    snapshots: BTreeMap<String, Snapshot>,
}

impl Store {
    pub fn new() -> Self {
        Self {
            clock: 0,
            revision: 0,
            versions: BTreeMap::new(),
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

    fn selected_version(&self, key: &str, tx: Option<u64>) -> Option<&Version> {
        let versions = self.versions.get(key)?;
        match tx {
            None => versions.last(),
            Some(id) => {
                let begin = self.transactions.get(&id)?.begin_revision;
                versions.iter().rev().find(|v| v.revision <= begin)
            }
        }
    }

    fn visible_write(&self, key: &str, tx: Option<u64>) -> Option<Write> {
        if let Some(id) = tx {
            self.transactions
                .get(&id)
                .and_then(|t| t.writes.get(key))
                .cloned()
        } else {
            None
        }
    }

    fn live_value(&self, key: &str, tx: Option<u64>, now: u64) -> Option<String> {
        if let Some(write) = self.visible_write(key, tx) {
            return match write.value {
                Some(value) if write.deadline.map_or(true, |d| d > now) => Some(value),
                _ => None,
            };
        }
        let v = self.selected_version(key, tx)?;
        match &v.value {
            Some(value) if v.deadline.map_or(true, |d| d > now) => Some(value.clone()),
            _ => None,
        }
    }

    fn append(&mut self, key: String, write: Write) {
        self.revision += 1;
        self.versions.entry(key).or_default().push(Version {
            revision: self.revision,
            value: write.value,
            deadline: write.deadline,
        });
    }

    fn validate_key(key: &str) -> Result<(), Error> {
        if key.is_empty() {
            Err(Error::KeyEmpty)
        } else {
            Ok(())
        }
    }

    fn validate_name(name: &str) -> Result<(), Error> {
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

    fn check_range(start: &str, end: Option<&str>) -> Result<(), Error> {
        if let Some(end) = end {
            if end <= start {
                return Err(Error::InvalidRange);
            }
        }
        Ok(())
    }

    fn visible_entries(&self, tx: Option<u64>, now: u64) -> BTreeMap<String, String> {
        let mut keys: BTreeSet<String> = self.versions.keys().cloned().collect();
        if let Some(id) = tx {
            if let Some(t) = self.transactions.get(&id) {
                keys.extend(t.writes.keys().cloned());
            }
        }
        keys.into_iter()
            .filter_map(|k| self.live_value(&k, tx, now).map(|v| (k, v)))
            .collect()
    }

    fn matches(k: &str, start: &str, end: Option<&str>, prefix: Option<&str>) -> bool {
        k >= start
            && end.map_or(true, |e| k < e)
            && prefix.map_or(true, |p| k.starts_with(p))
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
        for key in tx.writes.keys() {
            if self
                .versions
                .get(key)
                .and_then(|v| v.last())
                .map_or(false, |v| v.revision > tx.begin_revision)
            {
                return Err(Error::Conflict {
                    tx: id,
                    key: key.clone(),
                });
            }
        }

        let tx = self.transactions.remove(&id).unwrap();
        if tx.writes.is_empty() {
            return Ok(self.revision);
        }
        self.revision += 1;
        for (key, write) in tx.writes {
            self.versions.entry(key).or_default().push(Version {
                revision: self.revision,
                value: write.value,
                deadline: write.deadline,
            });
        }
        Ok(self.revision)
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

        let deadline = match ttl {
            Some(d) => Some(now.checked_add(d).ok_or(Error::DeadlineOverflow)?),
            None => None,
        };
        let write = if ttl == Some(0) {
            Write {
                value: None,
                deadline: None,
            }
        } else {
            Write {
                value: Some(value.to_string()),
                deadline,
            }
        };
        match tx {
            None => {
                self.append(key.to_string(), write);
            }
            Some(id) => {
                self.transactions
                    .get_mut(&id)
                    .unwrap()
                    .writes
                    .insert(key.to_string(), write);
            }
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
        let existed = self.live_value(key, tx, now).is_some();
        let write = Write {
            value: None,
            deadline: None,
        };
        match tx {
            None => self.append(key.to_string(), write),
            Some(id) => {
                self.transactions
                    .get_mut(&id)
                    .unwrap()
                    .writes
                    .insert(key.to_string(), write);
            }
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
        Ok(self.live_value(key, tx, now))
    }

    pub fn count(
        &mut self,
        now: u64,
        tx: Option<u64>,
        value: &str,
    ) -> Result<usize, Error> {
        self.admit(now)?;
        self.check_tx(tx)?;
        Ok(self
            .visible_entries(tx, now)
            .values()
            .filter(|v| v.as_str() == value)
            .count())
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
        Self::check_range(start, end)?;
        if let Some(c) = cursor {
            if c.start != start
                || c.end.as_deref() != end
                || c.prefix.as_deref() != prefix
                || c.offset != offset
                || c.last.is_empty()
                || !Self::matches(&c.last, start, end, prefix)
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

        let entries = self.visible_entries(tx, now);
        let mut keys: Vec<(String, String)> = entries
            .into_iter()
            .filter(|(k, _)| Self::matches(k, start, end, prefix))
            .collect();
        if let Some(c) = cursor {
            keys.retain(|(k, _)| k > &c.last);
        } else {
            keys = keys.into_iter().skip(offset).collect();
        }
        let has_more = keys.len() > limit;
        let items: Vec<(String, String)> = keys.into_iter().take(limit).collect();
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
        Self::check_range(start, end)?;
        let entries = self.visible_entries(tx, now);
        let mut count = 0usize;
        let mut sum = 0i64;
        for (key, value) in entries {
            if !Self::matches(&key, start, end, prefix) {
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

    pub fn snapshot(&mut self, now: u64, name: &str) -> Result<(), Error> {
        self.admit(now)?;
        Self::validate_name(name)?;
        if !self.snapshots.contains_key(name) && self.snapshots.len() >= 3 {
            return Err(Error::SnapshotLimit);
        }
        let entries = self
            .visible_entries(None, now)
            .into_iter()
            .filter_map(|(k, value)| {
                let v = self.versions.get(&k)?.last()?;
                Some((k, (value, v.deadline)))
            })
            .collect();
        self.snapshots
            .insert(name.to_string(), Snapshot { entries });
        Ok(())
    }

    pub fn restore(&mut self, now: u64, name: &str) -> Result<u64, Error> {
        self.admit(now)?;
        Self::validate_name(name)?;
        let saved = self
            .snapshots
            .get(name)
            .ok_or_else(|| Error::SnapshotMissing(name.to_string()))?;
        if !self.transactions.is_empty() {
            return Err(Error::ActiveTransactions);
        }

        let saved_live: BTreeMap<String, (String, Option<u64>)> = saved
            .entries
            .iter()
            .filter(|(_, (_, deadline))| deadline.map_or(true, |d| d > now))
            .map(|(k, v)| (k.clone(), v.clone()))
            .collect();

        let current = self.visible_entries(None, now);
        let mut union: BTreeSet<String> = current.keys().cloned().collect();
        union.extend(saved_live.keys().cloned());
        if union.is_empty() {
            return Ok(self.revision);
        }

        self.revision += 1;
        for key in union {
            let (value, deadline) = match saved_live.get(&key) {
                Some((value, deadline)) => (Some(value.clone()), *deadline),
                None => (None, None),
            };
            self.versions.entry(key).or_default().push(Version {
                revision: self.revision,
                value,
                deadline,
            });
        }
        Ok(self.revision)
    }

    pub fn forget(&mut self, now: u64, name: &str) -> Result<(), Error> {
        self.admit(now)?;
        Self::validate_name(name)?;
        if self.snapshots.remove(name).is_none() {
            return Err(Error::SnapshotMissing(name.to_string()));
        }
        Ok(())
    }

    pub fn gc(&mut self, now: u64) -> Result<usize, Error> {
        self.admit(now)?;
        let mut removed = 0usize;
        for versions in self.versions.values_mut() {
            if versions.len() <= 1 {
                continue;
            }
            let mut keep = BTreeSet::new();
            keep.insert(versions.len() - 1);
            for tx in self.transactions.values() {
                if let Some(index) = versions
                    .iter()
                    .rposition(|v| v.revision <= tx.begin_revision)
                {
                    keep.insert(index);
                }
            }
            let old = std::mem::take(versions);
            for (i, version) in old.into_iter().enumerate() {
                if keep.contains(&i) {
                    versions.push(version);
                } else {
                    removed += 1;
                }
            }
        }
        Ok(removed)
    }

    pub fn stats(&mut self, now: u64) -> Result<Stats, Error> {
        self.admit(now)?;
        let versions = self.versions.values().map(Vec::len).sum();
        let tombstones = self
            .versions
            .values()
            .flat_map(|v| v.iter())
            .filter(|v| v.value.is_none())
            .count();
        let live_keys = self.visible_entries(None, now).len();
        let read_only_transactions = self
            .transactions
            .values()
            .filter(|t| t.read_only)
            .count();
        let staged_writes = self
            .transactions
            .values()
            .map(|t| t.writes.len())
            .sum();
        Ok(Stats {
            clock: self.clock,
            revision: self.revision,
            keys: self.versions.len(),
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

    let digits = if bytes[0] == b'-' {
        if bytes.len() < 2 || bytes[1] == b'0' {
            return None;
        }
        &bytes[1..]
    } else {
        if bytes[0] == b'0' {
            return if bytes.len() == 1 { Some(0) } else { None };
        }
        bytes
    };

    if digits.is_empty() || !digits.iter().all(u8::is_ascii_digit) {
        return None;
    }
    value.parse::<i64>().ok()
}