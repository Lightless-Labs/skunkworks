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

pub struct Store {
    clock: u64,
    revision: u64,
    versions: BTreeMap<String, Vec<Version>>,
    transactions: HashMap<u64, Transaction>,
    snapshots: BTreeMap<String, BTreeMap<String, (String, Option<u64>)>>,
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

    fn transaction(&self, id: u64) -> Result<&Transaction, Error> {
        self.transactions
            .get(&id)
            .ok_or(Error::UnknownTransaction(id))
    }

    fn selected_version(&self, key: &str, tx: Option<u64>) -> Option<&Version> {
        let versions = self.versions.get(key)?;
        match tx {
            Some(id) => {
                let begin_revision = self.transactions.get(&id)?.begin_revision;
                versions
                    .iter()
                    .rev()
                    .find(|version| version.revision <= begin_revision)
            }
            None => versions.last(),
        }
    }

    fn live_value(&self, key: &str, tx: Option<u64>, now: u64) -> Option<&str> {
        if let Some(write) = tx
            .and_then(|id| self.transactions.get(&id))
            .and_then(|transaction| transaction.writes.get(key))
        {
            return match (&write.value, write.deadline) {
                (Some(value), deadline) if deadline.is_none_or(|d| d > now) => {
                    Some(value.as_str())
                }
                _ => None,
            };
        }

        let version = self.selected_version(key, tx)?;
        match (&version.value, version.deadline) {
            (Some(value), deadline) if deadline.is_none_or(|d| d > now) => {
                Some(value.as_str())
            }
            _ => None,
        }
    }

    fn append_version(&mut self, key: String, write: Write, revision: u64) {
        self.versions.entry(key).or_default().push(Version {
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

    fn validate_range(start: &str, end: Option<&str>) -> Result<(), Error> {
        if end.is_some_and(|end| end <= start) {
            Err(Error::InvalidRange)
        } else {
            Ok(())
        }
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
        let mut keys: BTreeSet<&str> = self.versions.keys().map(String::as_str).collect();
        if let Some(id) = tx {
            if let Some(transaction) = self.transactions.get(&id) {
                keys.extend(transaction.writes.keys().map(String::as_str));
            }
        }

        keys.into_iter()
            .filter(|key| {
                *key >= start
                    && end.is_none_or(|end| *key < end)
                    && prefix.is_none_or(|prefix| key.starts_with(prefix))
                    && after.is_none_or(|last| *key > last)
            })
            .filter_map(|key| {
                self.live_value(key, tx, now)
                    .map(|value| (key.to_owned(), value.to_owned()))
            })
            .collect()
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
        let transaction = self.transaction(id)?;

        for key in transaction.writes.keys() {
            if self
                .versions
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

        let transaction = self.transactions.remove(&id).unwrap();
        if transaction.writes.is_empty() {
            return Ok(self.revision);
        }

        self.revision += 1;
        let revision = self.revision;
        for (key, write) in transaction.writes {
            self.append_version(key, write, revision);
        }
        Ok(revision)
    }

    pub fn rollback(&mut self, now: u64, id: u64) -> Result<(), Error> {
        self.admit(now)?;
        self.transaction(id)?;
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

        if let Some(id) = tx {
            let transaction = self.transaction(id)?;
            Self::validate_key(key)?;
            if transaction.read_only {
                return Err(Error::ReadOnly(id));
            }
        } else {
            Self::validate_key(key)?;
        }

        let write = match ttl {
            Some(0) => Write {
                value: None,
                deadline: None,
            },
            Some(duration) => Write {
                value: Some(value.to_owned()),
                deadline: Some(now.checked_add(duration).ok_or(Error::DeadlineOverflow)?),
            },
            None => Write {
                value: Some(value.to_owned()),
                deadline: None,
            },
        };

        match tx {
            Some(id) => {
                self.transactions
                    .get_mut(&id)
                    .unwrap()
                    .writes
                    .insert(key.to_owned(), write);
            }
            None => {
                self.revision += 1;
                self.append_version(key.to_owned(), write, self.revision);
            }
        }
        Ok(())
    }

    pub fn delete(&mut self, now: u64, tx: Option<u64>, key: &str) -> Result<bool, Error> {
        self.admit(now)?;

        if let Some(id) = tx {
            let transaction = self.transaction(id)?;
            Self::validate_key(key)?;
            if transaction.read_only {
                return Err(Error::ReadOnly(id));
            }
        } else {
            Self::validate_key(key)?;
        }

        let existed = self.live_value(key, tx, now).is_some();
        let write = Write {
            value: None,
            deadline: None,
        };

        match tx {
            Some(id) => {
                self.transactions
                    .get_mut(&id)
                    .unwrap()
                    .writes
                    .insert(key.to_owned(), write);
            }
            None => {
                self.revision += 1;
                self.append_version(key.to_owned(), write, self.revision);
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
        if let Some(id) = tx {
            self.transaction(id)?;
        }
        Self::validate_key(key)?;
        Ok(self.live_value(key, tx, now).map(str::to_owned))
    }

    pub fn count(&mut self, now: u64, tx: Option<u64>, value: &str) -> Result<usize, Error> {
        self.admit(now)?;
        if let Some(id) = tx {
            self.transaction(id)?;
        }
        Ok(self
            .matching_keys(tx, now, "", None, None, None)
            .iter()
            .filter(|(_, found)| found == value)
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
        if let Some(id) = tx {
            self.transaction(id)?;
        }
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

        let all = self.matching_keys(
            tx,
            now,
            start,
            end,
            prefix,
            cursor.map(|cursor| cursor.last.as_str()),
        );
        let skip = if cursor.is_some() { 0 } else { offset };
        let items: Vec<_> = all.iter().skip(skip).take(limit).cloned().collect();
        let has_more = all.len().saturating_sub(skip) > items.len();

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
        if let Some(id) = tx {
            self.transaction(id)?;
        }
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
                .ok_or_else(|| Error::SumOverflow { key: key.clone() })?;
            count += 1;
        }
        Ok(Aggregate { count, sum })
    }

    fn parse_number(value: &str) -> Option<i64> {
        let bytes = value.as_bytes();
        if bytes.is_empty() {
            return None;
        }

        let digits = if bytes[0] == b'-' {
            &bytes[1..]
        } else {
            bytes
        };

        if digits.is_empty() || !digits.iter().all(u8::is_ascii_digit) {
            return None;
        }
        if (digits.len() > 1 && digits[0] == b'0')
            || (bytes[0] == b'-' && digits == b"0")
        {
            return None;
        }

        value.parse::<i64>().ok()
    }

    pub fn snapshot(&mut self, now: u64, name: &str) -> Result<(), Error> {
        self.admit(now)?;
        Self::validate_name(name)?;

        let mut saved = BTreeMap::new();
        for key in self.versions.keys() {
            if let Some(value) = self.live_value(key, None, now) {
                let deadline = self.versions[key]
                    .last()
                    .and_then(|version| version.deadline);
                saved.insert(key.clone(), (value.to_owned(), deadline));
            }
        }

        if !self.snapshots.contains_key(name) && self.snapshots.len() >= 3 {
            return Err(Error::SnapshotLimit);
        }
        self.snapshots.insert(name.to_owned(), saved);
        Ok(())
    }

    pub fn restore(&mut self, now: u64, name: &str) -> Result<u64, Error> {
        self.admit(now)?;
        if name.is_empty() {
            return Err(Error::NameEmpty);
        }
        Self::validate_name(name)?;

        let saved = self
            .snapshots
            .get(name)
            .cloned()
            .ok_or_else(|| Error::SnapshotMissing(name.to_owned()))?;

        if !self.transactions.is_empty() {
            return Err(Error::ActiveTransactions);
        }

        let copied: BTreeMap<_, _> = saved
            .into_iter()
            .filter(|(_, (_, deadline))| deadline.is_none_or(|deadline| deadline > now))
            .collect();

        let mut union = BTreeSet::new();
        for key in self.versions.keys() {
            if self.live_value(key, None, now).is_some() {
                union.insert(key.clone());
            }
        }
        union.extend(copied.keys().cloned());

        if union.is_empty() {
            return Ok(self.revision);
        }

        self.revision += 1;
        let revision = self.revision;
        for key in union {
            let write = match copied.get(&key) {
                Some((value, deadline)) => Write {
                    value: Some(value.clone()),
                    deadline: *deadline,
                },
                None => Write {
                    value: None,
                    deadline: None,
                },
            };
            self.append_version(key, write, revision);
        }
        Ok(revision)
    }

    pub fn forget(&mut self, now: u64, name: &str) -> Result<(), Error> {
        self.admit(now)?;
        if name.is_empty() {
            return Err(Error::NameEmpty);
        }
        Self::validate_name(name)?;

        if self.snapshots.remove(name).is_none() {
            return Err(Error::SnapshotMissing(name.to_owned()));
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

        let versions = self.versions.values().map(Vec::len).sum();
        let tombstones = self
            .versions
            .values()
            .flat_map(|versions| versions.iter())
            .filter(|version| version.value.is_none())
            .count();
        let live_keys = self
            .versions
            .keys()
            .filter(|key| self.live_value(key, None, now).is_some())
            .count();

        Ok(Stats {
            clock: self.clock,
            revision: self.revision,
            keys: self.versions.len(),
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

fn main() {}