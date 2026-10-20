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
    transactions: BTreeMap<u64, Transaction>,
    snapshots: BTreeMap<String, BTreeMap<String, Value>>,
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

    fn tx_exists(&self, tx: Option<u64>) -> Result<(), Error> {
        if let Some(id) = tx {
            if !self.transactions.contains_key(&id) {
                return Err(Error::UnknownTransaction(id));
            }
        }
        Ok(())
    }

    fn selected(&self, key: &str, revision: Option<u64>) -> Option<&Version> {
        let versions = self.data.get(key)?;
        match revision {
            None => versions.last(),
            Some(revision) => versions.iter().rev().find(|v| v.revision <= revision),
        }
    }

    fn live_value(value: Option<&Value>, now: u64) -> Option<&Value> {
        value.filter(|v| v.deadline.map_or(true, |deadline| deadline > now))
    }

    fn visible(&self, key: &str, tx: Option<u64>, now: u64) -> Option<&Value> {
        if let Some(id) = tx {
            let transaction = self.transactions.get(&id).unwrap();
            if let Some(write) = transaction.writes.get(key) {
                return Self::live_value(write.value.as_ref(), now);
            }
            let version = self.selected(key, Some(transaction.begin_revision))?;
            return Self::live_value(version.value.as_ref(), now);
        }

        let version = self.selected(key, None)?;
        Self::live_value(version.value.as_ref(), now)
    }

    fn append(&mut self, key: String, value: Option<Value>, revision: u64) {
        self.data
            .entry(key)
            .or_default()
            .push(Version { revision, value });
    }

    fn put(&mut self, tx: Option<u64>, key: String, value: Option<Value>) {
        if let Some(id) = tx {
            self.transactions
                .get_mut(&id)
                .unwrap()
                .writes
                .insert(key, Write { value });
        } else {
            self.revision += 1;
            self.append(key, value, self.revision);
        }
    }

    fn validate_name(name: &str) -> Result<(), Error> {
        if name.is_empty() {
            return Err(Error::NameEmpty);
        }

        for (i, byte) in name.bytes().enumerate() {
            let valid = if i == 0 {
                byte.is_ascii_lowercase()
            } else {
                byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'_'
            };
            if !valid {
                return Err(Error::NameInvalid { at: i });
            }
        }
        Ok(())
    }

    fn range_matches(
        key: &str,
        start: &str,
        end: Option<&str>,
        prefix: Option<&str>,
    ) -> bool {
        key >= start
            && end.map_or(true, |end| key < end)
            && prefix.map_or(true, |prefix| key.starts_with(prefix))
    }

    fn matching_keys(
        &self,
        tx: Option<u64>,
        now: u64,
        start: &str,
        end: Option<&str>,
        prefix: Option<&str>,
    ) -> Vec<String> {
        let mut keys = BTreeSet::new();
        for key in self.data.keys() {
            keys.insert(key.clone());
        }
        if let Some(id) = tx {
            for key in self.transactions.get(&id).unwrap().writes.keys() {
                keys.insert(key.clone());
            }
        }

        keys.into_iter()
            .filter(|key| {
                Self::range_matches(key, start, end, prefix)
                    && self.visible(key, tx, now).is_some()
            })
            .collect()
    }

    fn check_range(start: &str, end: Option<&str>) -> Result<(), Error> {
        if end.is_some_and(|end| end <= start) {
            Err(Error::InvalidRange)
        } else {
            Ok(())
        }
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
        let transaction = self
            .transactions
            .get(&id)
            .ok_or(Error::UnknownTransaction(id))?;

        if transaction.writes.is_empty() {
            self.transactions.remove(&id);
            return Ok(self.revision);
        }

        for key in transaction.writes.keys() {
            if self
                .data
                .get(key)
                .and_then(|versions| versions.last())
                .is_some_and(|version| version.revision > transaction.begin_revision)
            {
                return Err(Error::Conflict {
                    tx: id,
                    key: key.clone(),
                });
            }
        }

        let writes: Vec<(String, Option<Value>)> = transaction
            .writes
            .iter()
            .map(|(key, write)| (key.clone(), write.value.clone()))
            .collect();

        self.revision += 1;
        let revision = self.revision;
        for (key, value) in writes {
            self.append(key, value, revision);
        }
        self.transactions.remove(&id);
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
        self.tx_exists(tx)?;

        if key.is_empty() {
            return Err(Error::KeyEmpty);
        }
        if let Some(id) = tx {
            if self.transactions.get(&id).unwrap().read_only {
                return Err(Error::ReadOnly(id));
            }
        }

        let stored = match ttl {
            Some(0) => None,
            Some(duration) => Some(Value {
                value: value.to_string(),
                deadline: Some(
                    now.checked_add(duration)
                        .ok_or(Error::DeadlineOverflow)?,
                ),
            }),
            None => Some(Value {
                value: value.to_string(),
                deadline: None,
            }),
        };

        self.put(tx, key.to_string(), stored);
        Ok(())
    }

    pub fn delete(&mut self, now: u64, tx: Option<u64>, key: &str) -> Result<bool, Error> {
        self.admit(now)?;
        self.tx_exists(tx)?;

        if key.is_empty() {
            return Err(Error::KeyEmpty);
        }
        if let Some(id) = tx {
            if self.transactions.get(&id).unwrap().read_only {
                return Err(Error::ReadOnly(id));
            }
        }

        let existed = self.visible(key, tx, now).is_some();
        self.put(tx, key.to_string(), None);
        Ok(existed)
    }

    pub fn get(
        &mut self,
        now: u64,
        tx: Option<u64>,
        key: &str,
    ) -> Result<Option<String>, Error> {
        self.admit(now)?;
        self.tx_exists(tx)?;

        if key.is_empty() {
            return Err(Error::KeyEmpty);
        }
        Ok(self.visible(key, tx, now).map(|value| value.value.clone()))
    }

    pub fn count(&mut self, now: u64, tx: Option<u64>, value: &str) -> Result<usize, Error> {
        self.admit(now)?;
        self.tx_exists(tx)?;

        let keys = self.matching_keys(tx, now, "", None, None);
        Ok(keys
            .iter()
            .filter(|key| self.visible(key, tx, now).unwrap().value == value)
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
        self.tx_exists(tx)?;
        Self::check_range(start, end)?;

        if let Some(cursor) = cursor {
            if cursor.start != start
                || cursor.end.as_deref() != end
                || cursor.prefix.as_deref() != prefix
                || cursor.offset != offset
                || cursor.last.is_empty()
                || !Self::range_matches(&cursor.last, start, end, prefix)
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

        let keys = self.matching_keys(tx, now, start, end, prefix);
        let remaining: Vec<String> = if let Some(cursor) = cursor {
            keys.into_iter()
                .filter(|key| key.as_str() > cursor.last.as_str())
                .collect()
        } else {
            keys.into_iter().skip(offset).collect()
        };

        let has_more = remaining.len() > limit;
        let items: Vec<(String, String)> = remaining
            .iter()
            .take(limit)
            .map(|key| {
                (
                    key.clone(),
                    self.visible(key, tx, now).unwrap().value.clone(),
                )
            })
            .collect();

        let next = if has_more {
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

    pub fn aggregate(
        &mut self,
        now: u64,
        tx: Option<u64>,
        start: &str,
        end: Option<&str>,
        prefix: Option<&str>,
    ) -> Result<Aggregate, Error> {
        self.admit(now)?;
        self.tx_exists(tx)?;
        Self::check_range(start, end)?;

        let keys = self.matching_keys(tx, now, start, end, prefix);
        let mut count = 0usize;
        let mut sum = 0i64;

        for key in keys {
            let value = self.visible(&key, tx, now).unwrap().value.clone();
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

        let mut copy = BTreeMap::new();
        for (key, versions) in &self.data {
            if let Some(value) = versions
                .last()
                .and_then(|version| Self::live_value(version.value.as_ref(), now))
            {
                copy.insert(key.clone(), value.clone());
            }
        }
        self.snapshots.insert(name.to_string(), copy);
        Ok(())
    }

    pub fn restore(&mut self, now: u64, name: &str) -> Result<u64, Error> {
        self.admit(now)?;
        Self::validate_name(name)?;

        let saved = self
            .snapshots
            .get(name)
            .cloned()
            .ok_or_else(|| Error::SnapshotMissing(name.to_string()))?;

        if !self.transactions.is_empty() {
            return Err(Error::ActiveTransactions);
        }

        let live_saved: BTreeMap<String, Value> = saved
            .into_iter()
            .filter(|(_, value)| value.deadline.map_or(true, |deadline| deadline > now))
            .collect();

        let current: BTreeSet<String> = self
            .data
            .iter()
            .filter_map(|(key, versions)| {
                versions
                    .last()
                    .and_then(|version| Self::live_value(version.value.as_ref(), now))
                    .map(|_| key.clone())
            })
            .collect();

        let saved_keys: BTreeSet<String> = live_saved.keys().cloned().collect();
        let union: BTreeSet<String> = current.union(&saved_keys).cloned().collect();

        if union.is_empty() {
            return Ok(self.revision);
        }

        self.revision += 1;
        let revision = self.revision;
        for key in union {
            self.append(key.clone(), live_saved.get(&key).cloned(), revision);
        }
        Ok(revision)
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

        let begin_revisions: Vec<u64> = self
            .transactions
            .values()
            .map(|transaction| transaction.begin_revision)
            .collect();

        let mut removed = 0usize;
        for versions in self.data.values_mut() {
            let mut keep = BTreeSet::new();
            if !versions.is_empty() {
                keep.insert(versions.len() - 1);
            }
            for revision in &begin_revisions {
                if let Some(index) = versions.iter().rposition(|v| v.revision <= *revision) {
                    keep.insert(index);
                }
            }

            let mut index = 0usize;
            versions.retain(|_| {
                let should_keep = keep.contains(&index);
                index += 1;
                if !should_keep {
                    removed += 1;
                }
                should_keep
            });
        }
        Ok(removed)
    }

    pub fn stats(&mut self, now: u64) -> Result<Stats, Error> {
        self.admit(now)?;

        let keys = self.data.len();
        let versions: usize = self.data.values().map(Vec::len).sum();
        let tombstones = self
            .data
            .values()
            .flat_map(|versions| versions.iter())
            .filter(|version| version.value.is_none())
            .count();
        let live_keys = self
            .data
            .iter()
            .filter(|(_, versions)| {
                versions
                    .last()
                    .and_then(|version| Self::live_value(version.value.as_ref(), now))
                    .is_some()
            })
            .count();

        Ok(Stats {
            clock: self.clock,
            revision: self.revision,
            keys,
            live_keys,
            versions,
            tombstones,
            active_transactions: self.transactions.len(),
            read_only_transactions: self
                .transactions
                .values()
                .filter(|transaction| transaction.read_only)
                .count(),
            staged_writes: self
                .transactions
                .values()
                .map(|transaction| transaction.writes.len())
                .sum(),
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
        if bytes.len() < 2 {
            return None;
        }
        &bytes[1..]
    } else {
        bytes
    };

    if digits.iter().any(|byte| !byte.is_ascii_digit()) {
        return None;
    }
    if digits[0] == b'0' && digits.len() != 1 {
        return None;
    }
    if bytes[0] == b'-' && digits == b"0" {
        return None;
    }

    value.parse::<i64>().ok()
}