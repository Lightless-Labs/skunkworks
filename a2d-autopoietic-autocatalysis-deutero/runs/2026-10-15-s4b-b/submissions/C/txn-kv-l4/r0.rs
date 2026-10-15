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
    snapshots: BTreeMap<String, BTreeMap<String, SavedValue>>,
}

#[derive(Clone)]
struct Version {
    revision: u64,
    value: Option<String>,
    deadline: Option<u64>,
}

#[derive(Clone)]
struct SavedValue {
    value: String,
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

    fn selected(&self, key: &str, tx: Option<u64>) -> Option<Write> {
        if let Some(id) = tx {
            let t = self.transactions.get(&id)?;
            if let Some(w) = t.writes.get(key) {
                return Some(w.clone());
            }
            let versions = self.data.get(key)?;
            versions
                .iter()
                .rev()
                .find(|v| v.revision <= t.begin_revision)
                .map(|v| Write {
                    value: v.value.clone(),
                    deadline: v.deadline,
                })
        } else {
            self.data.get(key)?.last().map(|v| Write {
                value: v.value.clone(),
                deadline: v.deadline,
            })
        }
    }

    fn live_value(&self, key: &str, tx: Option<u64>, now: u64) -> Option<String> {
        let w = self.selected(key, tx)?;
        if w.deadline.map_or(false, |d| d <= now) {
            return None;
        }
        w.value
    }

    fn view(&self, tx: Option<u64>, now: u64) -> BTreeMap<String, String> {
        let mut keys: BTreeSet<String> = self.data.keys().cloned().collect();
        if let Some(id) = tx {
            if let Some(t) = self.transactions.get(&id) {
                keys.extend(t.writes.keys().cloned());
            }
        }
        keys.into_iter()
            .filter_map(|k| self.live_value(&k, tx, now).map(|v| (k, v)))
            .collect()
    }

    fn append(&mut self, key: String, write: Write, revision: u64) {
        self.data.entry(key).or_default().push(Version {
            revision,
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
        let bytes = name.as_bytes();
        if !(bytes[0].is_ascii_lowercase()) {
            return Err(Error::NameInvalid { at: 0 });
        }
        for (i, b) in bytes.iter().enumerate().skip(1) {
            if !(b.is_ascii_lowercase() || b.is_ascii_digit() || *b == b'_') {
                return Err(Error::NameInvalid { at: i });
            }
        }
        Ok(())
    }

    fn validate_range(start: &str, end: Option<&str>) -> Result<(), Error> {
        if let Some(e) = end {
            if e <= start {
                return Err(Error::InvalidRange);
            }
        }
        Ok(())
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
        let t = self
            .transactions
            .get(&id)
            .ok_or(Error::UnknownTransaction(id))?;

        for key in t.writes.keys() {
            if self
                .data
                .get(key)
                .and_then(|vs| vs.last())
                .map_or(false, |v| v.revision > t.begin_revision)
            {
                return Err(Error::Conflict {
                    tx: id,
                    key: key.clone(),
                });
            }
        }

        let t = self.transactions.remove(&id).unwrap();
        if t.writes.is_empty() {
            return Ok(self.revision);
        }
        self.revision += 1;
        let revision = self.revision;
        for (key, write) in t.writes {
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
                value: Some(value.to_owned()),
                deadline,
            }
        };
        if let Some(id) = tx {
            self.transactions
                .get_mut(&id)
                .unwrap()
                .writes
                .insert(key.to_owned(), write);
        } else {
            self.revision += 1;
            self.append(key.to_owned(), write, self.revision);
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
        if let Some(id) = tx {
            self.transactions
                .get_mut(&id)
                .unwrap()
                .writes
                .insert(key.to_owned(), write);
        } else {
            self.revision += 1;
            self.append(key.to_owned(), write, self.revision);
        }
        Ok(existed)
    }

    pub fn get(&mut self, now: u64, tx: Option<u64>, key: &str) -> Result<Option<String>, Error> {
        self.admit(now)?;
        self.check_tx(tx)?;
        Self::validate_key(key)?;
        Ok(self.live_value(key, tx, now))
    }

    pub fn count(&mut self, now: u64, tx: Option<u64>, value: &str) -> Result<usize, Error> {
        self.admit(now)?;
        self.check_tx(tx)?;
        Ok(self.view(tx, now).values().filter(|v| *v == value).count())
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

        let view = self.view(tx, now);
        let mut matching: Vec<(String, String)> = view
            .into_iter()
            .filter(|(k, _)| Self::matches(k, start, end, prefix))
            .filter(|(k, _)| cursor.map_or(true, |c| k.as_str() > c.last.as_str()))
            .collect();

        if cursor.is_none() {
            matching = matching.into_iter().skip(offset).collect();
        }

        let has_more = matching.len() > limit;
        let items: Vec<_> = matching.into_iter().take(limit).collect();
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
        Self::validate_range(start, end)?;
        let view = self.view(tx, now);
        let mut count = 0usize;
        let mut sum = 0i64;
        for (key, value) in view {
            if !Self::matches(&key, start, end, prefix) {
                continue;
            }
            let parsed = parse_number(&value).ok_or_else(|| Error::InvalidNumber {
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
        let replacing = self.snapshots.contains_key(name);
        if !replacing && self.snapshots.len() >= 3 {
            return Err(Error::SnapshotLimit);
        }
        let saved = self
            .view(None, now)
            .into_iter()
            .map(|(k, v)| {
                let deadline = self
                    .data
                    .get(&k)
                    .and_then(|vs| vs.last())
                    .and_then(|v| v.deadline);
                (k, SavedValue { value: v, deadline })
            })
            .collect();
        self.snapshots.insert(name.to_owned(), saved);
        Ok(())
    }

    pub fn restore(&mut self, now: u64, name: &str) -> Result<u64, Error> {
        self.admit(now)?;
        if name.is_empty() {
            return Err(Error::NameEmpty);
        }
        let saved = self
            .snapshots
            .get(name)
            .cloned()
            .ok_or_else(|| Error::SnapshotMissing(name.to_owned()))?;
        if !self.transactions.is_empty() {
            return Err(Error::ActiveTransactions);
        }
        let copied: BTreeMap<String, Write> = saved
            .into_iter()
            .filter(|(_, v)| v.deadline.map_or(true, |d| d > now))
            .map(|(k, v)| {
                (
                    k,
                    Write {
                        value: Some(v.value),
                        deadline: v.deadline,
                    },
                )
            })
            .collect();
        let current = self.view(None, now);
        let mut keys: BTreeSet<String> = current.keys().cloned().collect();
        keys.extend(copied.keys().cloned());
        if keys.is_empty() {
            return Ok(self.revision);
        }
        self.revision += 1;
        let revision = self.revision;
        for key in keys {
            let write = copied.get(&key).cloned().unwrap_or(Write {
                value: None,
                deadline: None,
            });
            self.append(key, write, revision);
        }
        Ok(revision)
    }

    pub fn forget(&mut self, now: u64, name: &str) -> Result<(), Error> {
        self.admit(now)?;
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
            let mut keep: BTreeSet<usize> = BTreeSet::new();
            if !versions.is_empty() {
                keep.insert(versions.len() - 1);
            }
            for tx in self.transactions.values() {
                if let Some(i) = versions.iter().rposition(|v| v.revision <= tx.begin_revision) {
                    keep.insert(i);
                }
            }
            let mut idx = 0usize;
            versions.retain(|_| {
                let retain = keep.contains(&idx);
                idx += 1;
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
            .flat_map(|vs| vs.iter())
            .filter(|v| v.value.is_none())
            .count();
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
    if s == "0" {
        return Some(0);
    }
    let digits = s.strip_prefix('-').unwrap_or(s);
    if digits.is_empty()
        || !digits.as_bytes()[0].is_ascii_digit()
        || digits.as_bytes()[0] == b'0'
        || !digits.bytes().all(|b| b.is_ascii_digit())
    {
        return None;
    }
    s.parse::<i64>().ok()
}

fn main() {}