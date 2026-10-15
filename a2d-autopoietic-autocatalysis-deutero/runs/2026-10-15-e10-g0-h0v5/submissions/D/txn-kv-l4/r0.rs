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
    data: BTreeMap<String, Vec<Version>>,
    transactions: HashMap<u64, Transaction>,
    snapshots: BTreeMap<String, Snapshot>,
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

    fn visible_version(&self, key: &str, tx: Option<u64>) -> Option<&Version> {
        let versions = self.data.get(key)?;
        let max_revision = tx
            .and_then(|id| self.transactions.get(&id))
            .map(|t| t.begin_revision);
        versions.iter().rev().find(|v| {
            max_revision
                .map(|r| v.revision <= r)
                .unwrap_or(true)
        })
    }

    fn visible_map(&self, tx: Option<u64>, now: u64) -> BTreeMap<String, String> {
        let mut result = BTreeMap::new();
        for key in self.data.keys() {
            let private = tx.and_then(|id| self.transactions.get(&id))
                .and_then(|t| t.writes.get(key));
            let selected = if let Some(w) = private {
                Some((w.value.as_ref(), w.deadline))
            } else {
                self.visible_version(key, tx)
                    .map(|v| (v.value.as_ref(), v.deadline))
            };
            if let Some((Some(value), deadline)) = selected {
                if deadline.map(|d| d > now).unwrap_or(true) {
                    result.insert(key.clone(), value.clone());
                }
            }
        }
        if let Some(id) = tx {
            if let Some(t) = self.transactions.get(&id) {
                for (key, w) in &t.writes {
                    if self.data.contains_key(key) {
                        continue;
                    }
                    if let (Some(value), live) =
                        (w.value.as_ref(), w.deadline.map(|d| d > now).unwrap_or(true))
                    {
                        if live {
                            result.insert(key.clone(), value.clone());
                        }
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

    fn valid_name(name: &str) -> Result<(), Error> {
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

    fn range_valid(start: &str, end: Option<&str>) -> Result<(), Error> {
        if end.map(|e| e <= start).unwrap_or(false) {
            Err(Error::InvalidRange)
        } else {
            Ok(())
        }
    }

    fn matches(key: &str, start: &str, end: Option<&str>, prefix: Option<&str>) -> bool {
        key >= start
            && end.map(|e| key < e).unwrap_or(true)
            && prefix.map(|p| key.starts_with(p)).unwrap_or(true)
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
                .and_then(|versions| versions.last())
                .map(|v| v.revision > tx.begin_revision)
                .unwrap_or(false)
            {
                return Err(Error::Conflict {
                    tx: id,
                    key: key.clone(),
                });
            }
        }

        if tx.writes.is_empty() {
            self.transactions.remove(&id);
            return Ok(self.revision);
        }

        let writes = self.transactions.remove(&id).unwrap().writes;
        self.revision += 1;
        let revision = self.revision;
        for (key, write) in writes {
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

        let write = match ttl {
            Some(0) => Write {
                value: None,
                deadline: None,
            },
            Some(d) => Write {
                value: Some(value.to_string()),
                deadline: Some(now.checked_add(d).ok_or(Error::DeadlineOverflow)?),
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
            self.append(key.to_string(), write, self.revision);
        }
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

        let currently_live = self
            .visible_map(tx, now)
            .get(key)
            .is_some();

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
            self.append(key.to_string(), write, self.revision);
        }
        Ok(currently_live)
    }

    pub fn get(&mut self, now: u64, tx: Option<u64>, key: &str) -> Result<Option<String>, Error> {
        self.admit(now)?;
        self.check_tx(tx)?;
        if key.is_empty() {
            return Err(Error::KeyEmpty);
        }
        Ok(self.visible_map(tx, now).remove(key))
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
        Self::range_valid(start, end)?;

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

        let mut keys: Vec<(String, String)> = self
            .visible_map(tx, now)
            .into_iter()
            .filter(|(k, _)| Self::matches(k, start, end, prefix))
            .filter(|(k, _)| cursor.map(|c| k.as_str() > c.last.as_str()).unwrap_or(true))
            .collect();

        if cursor.is_none() {
            if offset >= keys.len() {
                return Ok(Page {
                    items: Vec::new(),
                    next: None,
                });
            }
            keys.drain(..offset);
        }

        let has_more = keys.len() > limit;
        keys.truncate(limit);
        let next = if has_more {
            keys.last().map(|(last, _)| Cursor {
                start: start.to_string(),
                end: end.map(str::to_string),
                prefix: prefix.map(str::to_string),
                offset,
                last: last.clone(),
            })
        } else {
            None
        };
        Ok(Page { items: keys, next })
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
        Self::range_valid(start, end)?;

        let mut count = 0usize;
        let mut sum = 0i64;
        for (key, value) in self.visible_map(tx, now) {
            if !Self::matches(&key, start, end, prefix) {
                continue;
            }
            let parsed = Self::parse_number(&value).ok_or_else(|| Error::InvalidNumber {
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

    fn parse_number(value: &str) -> Option<i64> {
        if value.is_empty() || value == "-0" {
            return None;
        }
        let digits = if let Some(rest) = value.strip_prefix('-') {
            if rest.is_empty() { return None; }
            rest
        } else {
            value
        };
        if digits == "0" {
            return value.parse::<i64>().ok();
        }
        if digits.starts_with('0') || !digits.bytes().all(|b| b.is_ascii_digit()) {
            return None;
        }
        if value.starts_with('+') {
            return None;
        }
        value.parse::<i64>().ok()
    }

    pub fn snapshot(&mut self, now: u64, name: &str) -> Result<(), Error> {
        self.admit(now)?;
        if name.is_empty() {
            return Err(Error::NameEmpty);
        }
        Self::valid_name(name)?;

        if !self.snapshots.contains_key(name) && self.snapshots.len() >= 3 {
            return Err(Error::SnapshotLimit);
        }

        let mut entries = BTreeMap::new();
        for (key, value) in self.visible_map(None, now) {
            let deadline = self
                .visible_version(&key, None)
                .and_then(|v| v.deadline);
            entries.insert(key, (value, deadline));
        }
        self.snapshots
            .insert(name.to_string(), Snapshot { entries });
        Ok(())
    }

    pub fn restore(&mut self, now: u64, name: &str) -> Result<u64, Error> {
        self.admit(now)?;
        if name.is_empty() {
            return Err(Error::NameEmpty);
        }
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
            .filter(|(_, (_, deadline))| deadline.map(|d| d > now).unwrap_or(true))
            .map(|(k, (v, deadline))| {
                (
                    k,
                    Write {
                        value: Some(v),
                        deadline,
                    },
                )
            })
            .collect();

        let current = self.visible_map(None, now);
        let union: BTreeSet<String> = current
            .keys()
            .chain(saved.keys())
            .cloned()
            .collect();

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
        if name.is_empty() {
            return Err(Error::NameEmpty);
        }
        Self::valid_name(name)?;
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
            .map(|t| t.begin_revision)
            .collect();
        let mut removed = 0usize;

        for versions in self.data.values_mut() {
            if versions.len() <= 1 {
                continue;
            }
            let mut keep = BTreeSet::new();
            keep.insert(versions.len() - 1);
            for begin in &begin_revisions {
                if let Some(index) = versions.iter().rposition(|v| v.revision <= *begin) {
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
            .flat_map(|vs| vs.iter())
            .filter(|v| v.value.is_none())
            .count();
        let live_keys = self.visible_map(None, now).len();
        let active_transactions = self.transactions.len();
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
            keys: self.data.len(),
            live_keys,
            versions,
            tombstones,
            active_transactions,
            read_only_transactions,
            staged_writes,
            snapshots: self.snapshots.len(),
        })
    }
}