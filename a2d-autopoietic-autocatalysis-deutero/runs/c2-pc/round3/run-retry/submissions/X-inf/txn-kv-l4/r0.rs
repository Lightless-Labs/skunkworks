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
    revision: u64,
    read_only: bool,
    writes: BTreeMap<String, Write>,
}

#[derive(Clone)]
struct SnapshotEntry {
    value: String,
    deadline: Option<u64>,
}

pub struct Store {
    clock: u64,
    revision: u64,
    data: BTreeMap<String, Vec<Version>>,
    transactions: BTreeMap<u64, Transaction>,
    snapshots: BTreeMap<String, BTreeMap<String, SnapshotEntry>>,
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

    fn selected_version(&self, key: &str, tx: Option<u64>) -> Option<&Version> {
        let versions = self.data.get(key)?;
        let max_revision = tx
            .and_then(|id| self.transactions.get(&id).map(|t| t.revision));
        versions.iter().rev().find(|v| {
            max_revision.map_or(true, |r| v.revision <= r)
        })
    }

    fn visible_value(&self, key: &str, tx: Option<u64>, now: u64) -> Option<String> {
        if let Some(id) = tx {
            if let Some(write) = self.transactions.get(&id)?.writes.get(key) {
                return match (&write.value, write.deadline) {
                    (Some(v), Some(deadline)) if deadline <= now => None,
                    (Some(v), _) => Some(v.clone()),
                    (None, _) => None,
                };
            }
        }
        let version = self.selected_version(key, tx)?;
        match (&version.value, version.deadline) {
            (Some(v), Some(deadline)) if deadline <= now => None,
            (Some(v), _) => Some(v.clone()),
            (None, _) => None,
        }
    }

    fn committed_live(&self, now: u64) -> BTreeMap<String, SnapshotEntry> {
        let mut result = BTreeMap::new();
        for (key, versions) in &self.data {
            if let Some(v) = versions.last() {
                if let Some(value) = &v.value {
                    if v.deadline.map_or(true, |d| d > now) {
                        result.insert(
                            key.clone(),
                            SnapshotEntry {
                                value: value.clone(),
                                deadline: v.deadline,
                            },
                        );
                    }
                }
            }
        }
        result
    }

    fn append(&mut self, key: String, write: Write, revision: u64) {
        self.data.entry(key).or_default().push(Version {
            revision,
            value: write.value,
            deadline: write.deadline,
        });
    }

    fn put(&mut self, key: String, write: Write, tx: Option<u64>) {
        if let Some(id) = tx {
            self.transactions
                .get_mut(&id)
                .expect("validated transaction")
                .writes
                .insert(key, write);
        } else {
            self.revision += 1;
            self.append(key, write, self.revision);
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
                revision: self.revision,
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

        let begin_revision = tx.revision;
        for key in tx.writes.keys() {
            if self
                .data
                .get(key)
                .and_then(|versions| versions.last())
                .is_some_and(|v| v.revision > begin_revision)
            {
                return Err(Error::Conflict {
                    tx: id,
                    key: key.clone(),
                });
            }
        }

        let writes = self.transactions.get(&id).unwrap().writes.clone();
        self.revision += 1;
        let revision = self.revision;
        for (key, write) in writes {
            self.append(key, write, revision);
        }
        self.transactions.remove(&id);
        Ok(revision)
    }

    pub fn rollback(&mut self, now: u64, id: u64) -> Result<(), Error> {
        self.admit(now)?;
        if !self.transactions.contains_key(&id) {
            return Err(Error::UnknownTransaction(id));
        }
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
        self.admit(now)?;
        self.check_tx(tx)?;
        if key.is_empty() {
            return Err(Error::KeyEmpty);
        }
        if let Some(id) = tx {
            if self.transactions.get(&id).unwrap().read_only {
                return Err(Error::ReadOnly(id));
            }
        }
        let deadline = match ttl {
            Some(d) => Some(now.checked_add(d).ok_or(Error::DeadlineOverflow)?),
            None => None,
        };
        let write = if deadline == Some(now) {
            Write {
                value: None,
                deadline: None,
            }
        } else {
            Write {
                value: Some(value.to_owned()),
                deadline,
            }
        };
        self.put(key.to_owned(), write, tx);
        Ok(())
    }

    pub fn delete(&mut self, now: u64, tx: Option<u64>, key: &str) -> Result<bool, Error> {
        self.admit(now)?;
        self.check_tx(tx)?;
        if key.is_empty() {
            return Err(Error::KeyEmpty);
        }
        if let Some(id) = tx {
            if self.transactions.get(&id).unwrap().read_only {
                return Err(Error::ReadOnly(id));
            }
        }
        let existed = self.visible_value(key, tx, now).is_some();
        self.put(
            key.to_owned(),
            Write {
                value: None,
                deadline: None,
            },
            tx,
        );
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
        if key.is_empty() {
            return Err(Error::KeyEmpty);
        }
        Ok(self.visible_value(key, tx, now))
    }

    fn visible_map(&self, tx: Option<u64>, now: u64) -> BTreeMap<String, String> {
        let mut keys: BTreeSet<String> = self.data.keys().cloned().collect();
        if let Some(id) = tx {
            keys.extend(self.transactions[&id].writes.keys().cloned());
        }
        keys.into_iter()
            .filter_map(|key| {
                self.visible_value(&key, tx, now)
                    .map(|value| (key, value))
            })
            .collect()
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

    fn valid_key(key: &str, start: &str, end: Option<&str>, prefix: Option<&str>) -> bool {
        key >= start
            && end.map_or(true, |end| key < end)
            && prefix.map_or(true, |prefix| key.starts_with(prefix))
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
                || !Self::valid_key(&c.last, start, end, prefix)
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
        let mut matching: Vec<(String, String)> = map
            .into_iter()
            .filter(|(key, _)| Self::valid_key(key, start, end, prefix))
            .filter(|(key, _)| cursor.map_or(true, |c| key > &c.last))
            .collect();

        let skip = if cursor.is_some() { 0 } else { offset };
        if skip >= matching.len() {
            return Ok(Page {
                items: Vec::new(),
                next: None,
            });
        }
        matching.drain(0..skip);
        let has_more = matching.len() > limit;
        matching.truncate(limit);
        let next = if has_more {
            matching.last().map(|(last, _)| Cursor {
                start: start.to_owned(),
                end: end.map(str::to_owned),
                prefix: prefix.map(str::to_owned),
                offset,
                last: last.clone(),
            })
        } else {
            None
        };
        Ok(Page {
            items: matching,
            next,
        })
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
        let mut count = 0usize;
        let mut sum = 0i64;
        for (key, value) in self.visible_map(tx, now) {
            if !Self::valid_key(&key, start, end, prefix) {
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

    pub fn snapshot(&mut self, now: u64, name: &str) -> Result<(), Error> {
        self.admit(now)?;
        validate_name(name)?;
        if name.is_empty() {
            return Err(Error::NameEmpty);
        }
        if !self.snapshots.contains_key(name) && self.snapshots.len() >= 3 {
            return Err(Error::SnapshotLimit);
        }
        self.snapshots.insert(name.to_owned(), self.committed_live(now));
        Ok(())
    }

    pub fn restore(&mut self, now: u64, name: &str) -> Result<u64, Error> {
        self.admit(now)?;
        validate_name(name)?;
        if name.is_empty() {
            return Err(Error::NameEmpty);
        }
        let saved = self
            .snapshots
            .get(name)
            .ok_or_else(|| Error::SnapshotMissing(name.to_owned()))?;
        if !self.transactions.is_empty() {
            return Err(Error::ActiveTransactions);
        }

        let saved_live: BTreeMap<String, SnapshotEntry> = saved
            .iter()
            .filter(|(_, entry)| entry.deadline.map_or(true, |d| d > now))
            .map(|(key, entry)| (key.clone(), entry.clone()))
            .collect();
        let current = self.committed_live(now);
        let union: BTreeSet<String> = current
            .keys()
            .chain(saved_live.keys())
            .cloned()
            .collect();
        if union.is_empty() {
            return Ok(self.revision);
        }

        self.revision += 1;
        let revision = self.revision;
        for key in union {
            let write = match saved_live.get(&key) {
                Some(entry) => Write {
                    value: Some(entry.value.clone()),
                    deadline: entry.deadline,
                },
                None => Write {
                    value: None,
                    deadline: None,
                },
            };
            self.append(key, write, revision);
        }
        Ok(revision)
    }

    pub fn forget(&mut self, now: u64, name: &str) -> Result<(), Error> {
        self.admit(now)?;
        validate_name(name)?;
        if name.is_empty() {
            return Err(Error::NameEmpty);
        }
        if self.snapshots.remove(name).is_none() {
            return Err(Error::SnapshotMissing(name.to_owned()));
        }
        Ok(())
    }

    pub fn gc(&mut self, now: u64) -> Result<usize, Error> {
        self.admit(now)?;
        let mut removed = 0;
        for versions in self.data.values_mut() {
            let mut keep = BTreeSet::new();
            if !versions.is_empty() {
                keep.insert(versions.len() - 1);
            }
            for tx in self.transactions.values() {
                if let Some(index) = versions.iter().rposition(|v| v.revision <= tx.revision) {
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
        Ok(Stats {
            clock: self.clock,
            revision: self.revision,
            keys: self.data.values().filter(|v| !v.is_empty()).count(),
            live_keys: self.committed_live(now).len(),
            versions,
            tombstones,
            active_transactions: self.transactions.len(),
            read_only_transactions: self
                .transactions
                .values()
                .filter(|tx| tx.read_only)
                .count(),
            staged_writes: self
                .transactions
                .values()
                .map(|tx| tx.writes.len())
                .sum(),
            snapshots: self.snapshots.len(),
        })
    }
}

fn validate_name(name: &str) -> Result<(), Error> {
    if name.is_empty() {
        return Ok(());
    }
    for (at, byte) in name.bytes().enumerate() {
        let valid = if at == 0 {
            byte.is_ascii_lowercase()
        } else {
            byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'_'
        };
        if !valid {
            return Err(Error::NameInvalid { at });
        }
    }
    Ok(())
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
    if digits[0] == b'0' {
        if digits.len() != 1 || bytes[0] == b'-' {
            return None;
        }
    } else if !(b'1'..=b'9').contains(&digits[0]) {
        return None;
    }
    if !digits.iter().all(u8::is_ascii_digit) {
        return None;
    }
    value.parse::<i64>().ok()
}