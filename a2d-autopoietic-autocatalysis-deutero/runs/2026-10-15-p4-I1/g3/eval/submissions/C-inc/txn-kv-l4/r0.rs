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
struct Snapshot {
    entries: BTreeMap<String, (String, Option<u64>)>,
}

pub struct Store {
    clock: u64,
    revision: u64,
    data: BTreeMap<String, Vec<Version>>,
    transactions: BTreeMap<u64, Transaction>,
    snapshots: BTreeMap<String, Snapshot>,
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

    fn visible_write(&self, tx: Option<u64>, key: &str) -> Option<Write> {
        tx.and_then(|id| self.transactions.get(&id))
            .and_then(|t| t.writes.get(key))
            .cloned()
    }

    fn selected_version(&self, key: &str, tx: Option<u64>) -> Option<&Version> {
        let versions = self.data.get(key)?;
        match tx.and_then(|id| self.transactions.get(&id)) {
            Some(t) => versions
                .iter()
                .rev()
                .find(|v| v.revision <= t.begin_revision),
            None => versions.last(),
        }
    }

    fn visible_value(&self, key: &str, tx: Option<u64>, now: u64) -> Option<String> {
        if let Some(w) = self.visible_write(tx, key) {
            return match w.value {
                Some(value) if w.deadline.is_none_or(|d| d > now) => Some(value),
                _ => None,
            };
        }
        let v = self.selected_version(key, tx)?;
        match &v.value {
            Some(value) if v.deadline.is_none_or(|d| d > now) => Some(value.clone()),
            _ => None,
        }
    }

    fn append(&mut self, key: String, write: Write, revision: u64) {
        self.data.entry(key).or_default().push(Version {
            revision,
            value: write.value,
            deadline: write.deadline,
        });
    }

    fn write(&mut self, tx: Option<u64>, key: String, write: Write) {
        if let Some(id) = tx {
            self.transactions
                .get_mut(&id)
                .unwrap()
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

        let tx = self.transactions.remove(&id).unwrap();
        if tx.writes.is_empty() {
            return Ok(self.revision);
        }
        self.revision += 1;
        let revision = self.revision;
        for (key, write) in tx.writes {
            self.append(key, write, revision);
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
                value: Some(value.to_string()),
                deadline,
            }
        };
        self.write(tx, key.to_string(), write);
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
        self.write(
            tx,
            key.to_string(),
            Write {
                value: None,
                deadline: None,
            },
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

    fn matching_keys(
        &self,
        tx: Option<u64>,
        now: u64,
        start: &str,
        end: Option<&str>,
        prefix: Option<&str>,
        after: Option<&str>,
    ) -> Vec<(String, String)> {
        let mut keys: BTreeSet<String> = self.data.keys().cloned().collect();
        if let Some(id) = tx {
            keys.extend(self.transactions[&id].writes.keys().cloned());
        }
        keys.into_iter()
            .filter(|key| {
                key.as_str() >= start
                    && end.is_none_or(|e| key.as_str() < e)
                    && prefix.is_none_or(|p| key.starts_with(p))
                    && after.is_none_or(|a| key.as_str() > a)
            })
            .filter_map(|key| {
                self.visible_value(&key, tx, now)
                    .map(|value| (key, value))
            })
            .collect()
    }

    fn validate_range(start: &str, end: Option<&str>) -> Result<(), Error> {
        if end.is_some_and(|e| e <= start) {
            return Err(Error::InvalidRange);
        }
        Ok(())
    }

    pub fn count(&mut self, now: u64, tx: Option<u64>, value: &str) -> Result<usize, Error> {
        self.admit(now)?;
        self.check_tx(tx)?;
        let keys = self.matching_keys(tx, now, "", None, None, None);
        Ok(keys.iter().filter(|(_, v)| v == value).count())
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
        Self::validate_range(start, end)?;
        if let Some(c) = cursor {
            if c.start != start
                || c.end.as_deref() != end
                || c.prefix.as_deref() != prefix
                || c.offset != offset
                || c.last.is_empty()
                || c.last.as_str() < start
                || end.is_some_and(|e| c.last.as_str() >= e)
                || prefix.is_some_and(|p| !c.last.starts_with(p))
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

        let all = self.matching_keys(
            tx,
            now,
            start,
            end,
            prefix,
            cursor.map(|c| c.last.as_str()),
        );
        let skip = if cursor.is_some() { 0 } else { offset };
        let items: Vec<_> = all.iter().skip(skip).take(limit).cloned().collect();
        let consumed = skip.saturating_add(items.len());
        let next = if !items.is_empty() && consumed < all.len() {
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
        self.check_tx(tx)?;
        Self::validate_range(start, end)?;
        let mut count = 0usize;
        let mut sum = 0i64;
        for (key, value) in self.matching_keys(tx, now, start, end, prefix, None) {
            let parsed = Self::parse_number(&value).ok_or_else(|| Error::InvalidNumber {
                key: key.clone(),
                value: value.clone(),
            })?;
            sum = sum
                .checked_add(parsed)
                .ok_or_else(|| Error::SumOverflow { key })?;
            count += 1;
        }
        Ok(Aggregate { count, sum })
    }

    fn parse_number(value: &str) -> Option<i64> {
        if value == "0" {
            return Some(0);
        }
        let digits = value.strip_prefix('-').unwrap_or(value);
        if digits.is_empty()
            || digits.starts_with('0')
            || !digits.bytes().all(|b| b.is_ascii_digit())
            || value == "-0"
            || value.starts_with('+')
        {
            return None;
        }
        value.parse::<i64>().ok()
    }

    fn valid_name(name: &str) -> Result<(), Error> {
        if name.is_empty() {
            return Err(Error::NameEmpty);
        }
        for (i, ch) in name.char_indices() {
            let ok = if i == 0 {
                ch.is_ascii_lowercase()
            } else {
                ch.is_ascii_lowercase() || ch.is_ascii_digit() || ch == '_'
            };
            if !ok {
                return Err(Error::NameInvalid { at: i });
            }
        }
        Ok(())
    }

    pub fn snapshot(&mut self, now: u64, name: &str) -> Result<(), Error> {
        self.admit(now)?;
        Self::valid_name(name)?;
        if !self.snapshots.contains_key(name) && self.snapshots.len() >= 3 {
            return Err(Error::SnapshotLimit);
        }
        let mut entries = BTreeMap::new();
        for key in self.data.keys() {
            if let Some(v) = self.visible_value(key, None, now) {
                let version = self.data[key].last().unwrap();
                entries.insert(key.clone(), (v, version.deadline));
            }
        }
        self.snapshots
            .insert(name.to_string(), Snapshot { entries });
        Ok(())
    }

    pub fn restore(&mut self, now: u64, name: &str) -> Result<u64, Error> {
        self.admit(now)?;
        Self::valid_name(name)?;
        let snapshot = self
            .snapshots
            .get(name)
            .cloned()
            .ok_or_else(|| Error::SnapshotMissing(name.to_string()))?;
        if !self.transactions.is_empty() {
            return Err(Error::ActiveTransactions);
        }
        let saved: BTreeMap<String, Write> = snapshot
            .entries
            .into_iter()
            .filter(|(_, (_, deadline))| deadline.is_none_or(|d| d > now))
            .map(|(k, (v, d))| {
                (
                    k,
                    Write {
                        value: Some(v),
                        deadline: d,
                    },
                )
            })
            .collect();
        let mut union: BTreeSet<String> = saved.keys().cloned().collect();
        for key in self.data.keys() {
            if self.visible_value(key, None, now).is_some() {
                union.insert(key.clone());
            }
        }
        if union.is_empty() {
            return Ok(self.revision);
        }
        self.revision += 1;
        let revision = self.revision;
        for key in union {
            let write = saved.get(&key).cloned().unwrap_or(Write {
                value: None,
                deadline: None,
            });
            self.append(key, write, revision);
        }
        Ok(revision)
    }

    pub fn forget(&mut self, now: u64, name: &str) -> Result<(), Error> {
        self.admit(now)?;
        Self::valid_name(name)?;
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
            if !versions.is_empty() {
                keep.insert(versions.len() - 1);
            }
            for tx in self.transactions.values() {
                if let Some((index, _)) = versions
                    .iter()
                    .enumerate()
                    .rev()
                    .find(|(_, v)| v.revision <= tx.begin_revision)
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
        let live_keys = self
            .data
            .keys()
            .filter(|key| self.visible_value(key, None, now).is_some())
            .count();
        Ok(Stats {
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
            staged_writes: self
                .transactions
                .values()
                .map(|t| t.writes.len())
                .sum(),
            snapshots: self.snapshots.len(),
        })
    }
}

fn main() {}