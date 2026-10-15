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
struct SavedValue {
    value: String,
    deadline: Option<u64>,
}

pub struct Store {
    clock: u64,
    revision: u64,
    data: BTreeMap<String, Vec<Version>>,
    transactions: BTreeMap<u64, Transaction>,
    snapshots: BTreeMap<String, BTreeMap<String, SavedValue>>,
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

    fn admit_time(&mut self, now: u64) -> Result<(), Error> {
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

    fn validate_key(key: &str) -> Result<(), Error> {
        if key.is_empty() {
            Err(Error::KeyEmpty)
        } else {
            Ok(())
        }
    }

    fn latest_at(&self, key: &str, revision: Option<u64>) -> Option<&Version> {
        self.data.get(key).and_then(|versions| match revision {
            Some(r) => versions.iter().rev().find(|v| v.revision <= r),
            None => versions.last(),
        })
    }

    fn visible_value(&self, key: &str, tx: Option<u64>, now: u64) -> Option<String> {
        if let Some(id) = tx {
            let transaction = self.transactions.get(&id)?;
            if let Some(write) = transaction.writes.get(key) {
                return Self::live_value(&write.value, write.deadline, now);
            }
            let version = self.latest_at(key, Some(transaction.begin_revision))?;
            return Self::live_value(&version.value, version.deadline, now);
        }
        let version = self.latest_at(key, None)?;
        Self::live_value(&version.value, version.deadline, now)
    }

    fn live_value(value: &Option<String>, deadline: Option<u64>, now: u64) -> Option<String> {
        if deadline.is_some_and(|d| d <= now) {
            return None;
        }
        value.clone()
    }

    fn visible_map(&self, tx: Option<u64>, now: u64) -> BTreeMap<String, String> {
        let mut result = BTreeMap::new();
        for key in self.data.keys() {
            if let Some(value) = self.visible_value(key, tx, now) {
                result.insert(key.clone(), value);
            }
        }
        if let Some(id) = tx {
            if let Some(transaction) = self.transactions.get(&id) {
                for (key, write) in &transaction.writes {
                    if let Some(value) = Self::live_value(&write.value, write.deadline, now) {
                        result.insert(key.clone(), value);
                    } else {
                        result.remove(key);
                    }
                }
            }
        }
        result
    }

    fn append_version(&mut self, key: String, write: Write, revision: u64) {
        self.data.entry(key).or_default().push(Version {
            revision,
            value: write.value,
            deadline: write.deadline,
        });
    }

    pub fn begin(&mut self, now: u64, id: u64, read_only: bool) -> Result<(), Error> {
        self.admit_time(now)?;
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
        self.admit_time(now)?;
        self.check_tx(Some(id))?;

        let (begin_revision, staged) = {
            let transaction = self.transactions.get(&id).unwrap();
            (transaction.begin_revision, transaction.writes.clone())
        };

        if staged.is_empty() {
            self.transactions.remove(&id);
            return Ok(self.revision);
        }

        for key in staged.keys() {
            if self
                .latest_at(key, None)
                .is_some_and(|version| version.revision > begin_revision)
            {
                return Err(Error::Conflict {
                    tx: id,
                    key: key.clone(),
                });
            }
        }

        self.revision += 1;
        let revision = self.revision;
        for (key, write) in staged {
            self.append_version(key, write, revision);
        }
        self.transactions.remove(&id);
        Ok(revision)
    }

    pub fn rollback(&mut self, now: u64, id: u64) -> Result<(), Error> {
        self.admit_time(now)?;
        self.check_tx(Some(id))?;
        self.transactions.remove(&id);
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
        self.admit_time(now)?;
        self.check_tx(tx)?;
        Self::validate_key(key)?;
        if let Some(id) = tx {
            if self.transactions.get(&id).unwrap().read_only {
                return Err(Error::ReadOnly(id));
            }
        }

        let write = match ttl {
            Some(0) => Write {
                value: None,
                deadline: None,
            },
            Some(duration) => Write {
                value: Some(value.to_string()),
                deadline: Some(now.checked_add(duration).ok_or(Error::DeadlineOverflow)?),
            },
            None => Write {
                value: Some(value.to_string()),
                deadline: None,
            },
        };

        if let Some(id) = tx {
            self.transactions
                .get_mut(&id)
                .unwrap()
                .writes
                .insert(key.to_string(), write);
        } else {
            self.revision += 1;
            let revision = self.revision;
            self.append_version(key.to_string(), write, revision);
        }
        Ok(())
    }

    pub fn delete(&mut self, now: u64, tx: Option<u64>, key: &str) -> Result<bool, Error> {
        self.admit_time(now)?;
        self.check_tx(tx)?;
        Self::validate_key(key)?;
        if let Some(id) = tx {
            if self.transactions.get(&id).unwrap().read_only {
                return Err(Error::ReadOnly(id));
            }
        }

        let existed = self.visible_value(key, tx, now).is_some();
        let write = Write {
            value: None,
            deadline: None,
        };
        if let Some(id) = tx {
            self.transactions
                .get_mut(&id)
                .unwrap()
                .writes
                .insert(key.to_string(), write);
        } else {
            self.revision += 1;
            let revision = self.revision;
            self.append_version(key.to_string(), write, revision);
        }
        Ok(existed)
    }

    pub fn get(
        &mut self,
        now: u64,
        tx: Option<u64>,
        key: &str,
    ) -> Result<Option<String>, Error> {
        self.admit_time(now)?;
        self.check_tx(tx)?;
        Self::validate_key(key)?;
        Ok(self.visible_value(key, tx, now))
    }

    pub fn count(
        &mut self,
        now: u64,
        tx: Option<u64>,
        value: &str,
    ) -> Result<usize, Error> {
        self.admit_time(now)?;
        self.check_tx(tx)?;
        Ok(self
            .visible_map(tx, now)
            .values()
            .filter(|v| v.as_str() == value)
            .count())
    }

    fn range_valid(start: &str, end: Option<&str>) -> bool {
        end.is_none_or(|e| e > start)
    }

    fn matches(key: &str, start: &str, end: Option<&str>, prefix: Option<&str>) -> bool {
        key >= start
            && end.is_none_or(|e| key < e)
            && prefix.is_none_or(|p| key.starts_with(p))
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
        self.admit_time(now)?;
        self.check_tx(tx)?;
        if !Self::range_valid(start, end) {
            return Err(Error::InvalidRange);
        }
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

        let visible = self.visible_map(tx, now);
        let mut keys: Vec<&String> = visible
            .keys()
            .filter(|key| Self::matches(key, start, end, prefix))
            .filter(|key| cursor.is_none_or(|c| key.as_str() > c.last.as_str()))
            .collect();

        if cursor.is_none() {
            keys = keys.into_iter().skip(offset).collect();
        }

        let has_more = keys.len() > limit;
        let items: Vec<(String, String)> = keys
            .iter()
            .take(limit)
            .map(|key| ((*key).clone(), visible.get(*key).unwrap().clone()))
            .collect();

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
        self.admit_time(now)?;
        self.check_tx(tx)?;
        if !Self::range_valid(start, end) {
            return Err(Error::InvalidRange);
        }

        let visible = self.visible_map(tx, now);
        let mut count = 0usize;
        let mut sum = 0i64;
        for (key, value) in visible {
            if !Self::matches(&key, start, end, prefix) {
                continue;
            }
            let number = parse_canonical_i64(&value).ok_or_else(|| Error::InvalidNumber {
                key: key.clone(),
                value: value.clone(),
            })?;
            sum = sum
                .checked_add(number)
                .ok_or_else(|| Error::SumOverflow { key: key.clone() })?;
            count += 1;
        }
        Ok(Aggregate { count, sum })
    }

    fn validate_name(name: &str) -> Result<(), Error> {
        if name.is_empty() {
            return Err(Error::NameEmpty);
        }
        for (at, ch) in name.char_indices() {
            let valid = if at == 0 {
                ch.is_ascii_lowercase()
            } else {
                ch.is_ascii_lowercase() || ch.is_ascii_digit() || ch == '_'
            };
            if !valid {
                return Err(Error::NameInvalid { at });
            }
        }
        Ok(())
    }

    pub fn snapshot(&mut self, now: u64, name: &str) -> Result<(), Error> {
        self.admit_time(now)?;
        if name.is_empty() {
            return Err(Error::NameEmpty);
        }
        Self::validate_name(name)?;
        if !self.snapshots.contains_key(name) && self.snapshots.len() >= 3 {
            return Err(Error::SnapshotLimit);
        }

        let mut saved = BTreeMap::new();
        for (key, version) in &self.data {
            if let Some(version) = version.last() {
                if let Some(value) = Self::live_value(&version.value, version.deadline, now) {
                    saved.insert(
                        key.clone(),
                        SavedValue {
                            value,
                            deadline: version.deadline,
                        },
                    );
                }
            }
        }
        self.snapshots.insert(name.to_string(), saved);
        Ok(())
    }

    pub fn restore(&mut self, now: u64, name: &str) -> Result<u64, Error> {
        self.admit_time(now)?;
        if name.is_empty() {
            return Err(Error::NameEmpty);
        }
        Self::validate_name(name)?;
        let saved = self
            .snapshots
            .get(name)
            .cloned()
            .ok_or_else(|| Error::SnapshotMissing(name.to_string()))?;
        if !self.transactions.is_empty() {
            return Err(Error::ActiveTransactions);
        }

        let copied: BTreeMap<String, SavedValue> = saved
            .into_iter()
            .filter(|(_, entry)| entry.deadline.is_none_or(|d| d > now))
            .collect();

        let current = self.visible_map(None, now);
        let mut union: BTreeSet<String> = current.keys().cloned().collect();
        union.extend(copied.keys().cloned());
        if union.is_empty() {
            return Ok(self.revision);
        }

        self.revision += 1;
        let revision = self.revision;
        for key in union {
            let write = if let Some(entry) = copied.get(&key) {
                Write {
                    value: Some(entry.value.clone()),
                    deadline: entry.deadline,
                }
            } else {
                Write {
                    value: None,
                    deadline: None,
                }
            };
            self.append_version(key, write, revision);
        }
        Ok(revision)
    }

    pub fn forget(&mut self, now: u64, name: &str) -> Result<(), Error> {
        self.admit_time(now)?;
        if name.is_empty() {
            return Err(Error::NameEmpty);
        }
        Self::validate_name(name)?;
        if self.snapshots.remove(name).is_none() {
            return Err(Error::SnapshotMissing(name.to_string()));
        }
        Ok(())
    }

    pub fn gc(&mut self, now: u64) -> Result<usize, Error> {
        self.admit_time(now)?;
        let mut removed = 0usize;

        for versions in self.data.values_mut() {
            let mut keep = BTreeSet::new();
            if !versions.is_empty() {
                keep.insert(versions.len() - 1);
            }
            for transaction in self.transactions.values() {
                if let Some(index) = versions
                    .iter()
                    .rposition(|v| v.revision <= transaction.begin_revision)
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
        self.admit_time(now)?;
        let keys = self.data.len();
        let versions = self.data.values().map(Vec::len).sum();
        let tombstones = self
            .data
            .values()
            .flat_map(|v| v.iter())
            .filter(|v| v.value.is_none())
            .count();
        let live_keys = self.visible_map(None, now).len();
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
            keys,
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
        if bytes.len() < 2 {
            return None;
        }
        &bytes[1..]
    } else {
        bytes
    };

    if digits.is_empty() || !digits.iter().all(u8::is_ascii_digit) {
        return None;
    }
    if digits.len() > 1 && digits[0] == b'0' {
        return None;
    }
    if bytes[0] == b'-' && digits == b"0" {
        return None;
    }
    if bytes[0] == b'0' && bytes.len() > 1 {
        return None;
    }
    value.parse::<i64>().ok()
}