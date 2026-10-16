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

struct Transaction {
    begin_revision: u64,
    read_only: bool,
    writes: BTreeMap<String, Option<Value>>,
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

    fn check_tx(&self, tx: Option<u64>) -> Result<(), Error> {
        if let Some(id) = tx {
            if !self.transactions.contains_key(&id) {
                return Err(Error::UnknownTransaction(id));
            }
        }
        Ok(())
    }

    fn selected_value(&self, key: &str, tx: Option<u64>) -> Option<Value> {
        if let Some(id) = tx {
            let transaction = self.transactions.get(&id)?;
            if let Some(value) = transaction.writes.get(key) {
                return value.clone();
            }
            let begin_revision = transaction.begin_revision;
            return self
                .data
                .get(key)?
                .iter()
                .rev()
                .find(|version| version.revision <= begin_revision)?
                .value
                .clone();
        }
        self.data.get(key)?.last()?.value.clone()
    }

    fn visible_live(&self, key: &str, tx: Option<u64>, now: u64) -> Option<String> {
        let value = self.selected_value(key, tx)?;
        if value.deadline.is_some_and(|deadline| deadline <= now) {
            None
        } else {
            Some(value.value)
        }
    }

    fn visible_entries(&self, tx: Option<u64>, now: u64) -> Vec<(String, String)> {
        let keys: BTreeSet<String> = self
            .data
            .keys()
            .cloned()
            .chain(
                tx.and_then(|id| self.transactions.get(&id))
                    .into_iter()
                    .flat_map(|transaction| transaction.writes.keys().cloned()),
            )
            .collect();

        keys.into_iter()
            .filter_map(|key| {
                self.visible_live(&key, tx, now)
                    .map(|value| (key, value))
            })
            .collect()
    }

    fn append_committed(&mut self, key: String, value: Option<Value>) {
        self.revision += 1;
        self.append_at_revision(key, value, self.revision);
    }

    fn append_at_revision(&mut self, key: String, value: Option<Value>, revision: u64) {
        self.data
            .entry(key)
            .or_default()
            .push(Version { revision, value });
    }

    fn valid_name(name: &str) -> Result<(), Error> {
        if name.is_empty() {
            return Err(Error::NameEmpty);
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

    fn validate_range(start: &str, end: Option<&str>) -> Result<(), Error> {
        if end.is_some_and(|value| value <= start) {
            return Err(Error::InvalidRange);
        }
        Ok(())
    }

    fn matching_entries(
        &self,
        tx: Option<u64>,
        now: u64,
        start: &str,
        end: Option<&str>,
        prefix: Option<&str>,
    ) -> Vec<(String, String)> {
        self.visible_entries(tx, now)
            .into_iter()
            .filter(|(key, _)| {
                key.as_str() >= start
                    && end.is_none_or(|end| key.as_str() < end)
                    && prefix.is_none_or(|prefix| key.starts_with(prefix))
            })
            .collect()
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

        if !transaction.writes.is_empty() {
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
        }

        let transaction = self.transactions.remove(&id).unwrap();
        if transaction.writes.is_empty() {
            return Ok(self.revision);
        }

        self.revision += 1;
        let revision = self.revision;
        for (key, value) in transaction.writes {
            self.append_at_revision(key, value, revision);
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

        let staged = match ttl {
            Some(0) => None,
            Some(duration) => Some(Value {
                value: value.to_owned(),
                deadline: Some(now.checked_add(duration).ok_or(Error::DeadlineOverflow)?),
            }),
            None => Some(Value {
                value: value.to_owned(),
                deadline: None,
            }),
        };

        if let Some(id) = tx {
            self.transactions
                .get_mut(&id)
                .unwrap()
                .writes
                .insert(key.to_owned(), staged);
        } else {
            self.append_committed(key.to_owned(), staged);
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

        let existed = self.visible_live(key, tx, now).is_some();
        if let Some(id) = tx {
            self.transactions
                .get_mut(&id)
                .unwrap()
                .writes
                .insert(key.to_owned(), None);
        } else {
            self.append_committed(key.to_owned(), None);
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
        if key.is_empty() {
            return Err(Error::KeyEmpty);
        }
        Ok(self.visible_live(key, tx, now))
    }

    pub fn count(&mut self, now: u64, tx: Option<u64>, value: &str) -> Result<usize, Error> {
        self.admit(now)?;
        self.check_tx(tx)?;
        Ok(self
            .visible_entries(tx, now)
            .into_iter()
            .filter(|(_, item)| item == value)
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
        Self::validate_range(start, end)?;

        if let Some(cursor) = cursor {
            if cursor.start != start
                || cursor.end.as_deref() != end
                || cursor.prefix.as_deref() != prefix
                || cursor.offset != offset
                || cursor.last.is_empty()
                || cursor.last.as_str() < start
                || end.is_some_and(|end| cursor.last.as_str() >= end)
                || prefix.is_some_and(|prefix| !cursor.last.starts_with(prefix))
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

        let mut entries = self.matching_entries(tx, now, start, end, prefix);
        if let Some(cursor) = cursor {
            entries.retain(|(key, _)| key > &cursor.last);
        } else {
            entries = entries.into_iter().skip(offset).collect();
        }

        let has_more = entries.len() > limit;
        let items: Vec<_> = entries.into_iter().take(limit).collect();
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

        let mut count = 0usize;
        let mut sum = 0i64;
        for (key, value) in self.matching_entries(tx, now, start, end, prefix) {
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
        Self::valid_name(name)?;
        if !self.snapshots.contains_key(name) && self.snapshots.len() >= 3 {
            return Err(Error::SnapshotLimit);
        }

        let snapshot = self
            .visible_entries(None, now)
            .into_iter()
            .filter_map(|(key, _)| {
                let value = self.selected_value(&key, None)?;
                if value.deadline.is_some_and(|deadline| deadline <= now) {
                    None
                } else {
                    Some((key, value))
                }
            })
            .collect();
        self.snapshots.insert(name.to_owned(), snapshot);
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
            .ok_or_else(|| Error::SnapshotMissing(name.to_owned()))?;
        if !self.transactions.is_empty() {
            return Err(Error::ActiveTransactions);
        }

        let saved_live: BTreeMap<String, Value> = saved
            .iter()
            .filter(|(_, value)| value.deadline.is_none_or(|deadline| deadline > now))
            .map(|(key, value)| (key.clone(), value.clone()))
            .collect();

        let current_live: BTreeSet<String> = self
            .data
            .keys()
            .filter(|key| self.visible_live(key, None, now).is_some())
            .cloned()
            .collect();

        let keys: BTreeSet<String> = current_live
            .into_iter()
            .chain(saved_live.keys().cloned())
            .collect();
        if keys.is_empty() {
            return Ok(self.revision);
        }

        self.revision += 1;
        let revision = self.revision;
        for key in keys {
            self.append_at_revision(
                key.clone(),
                saved_live.get(&key).cloned(),
                revision,
            );
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
            let mut keep = BTreeSet::new();
            if !versions.is_empty() {
                keep.insert(versions.len() - 1);
            }
            for transaction in self.transactions.values() {
                if let Some(index) = versions
                    .iter()
                    .rposition(|version| version.revision <= transaction.begin_revision)
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
            .flat_map(|versions| versions.iter())
            .filter(|version| version.value.is_none())
            .count();
        let live_keys = self
            .data
            .keys()
            .filter(|key| self.visible_live(key, None, now).is_some())
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

    if !digits.iter().all(u8::is_ascii_digit) {
        return None;
    }

    let parsed = value.parse::<i64>().ok()?;
    if parsed == 0 && value.starts_with('-') {
        return None;
    }
    Some(parsed)
}