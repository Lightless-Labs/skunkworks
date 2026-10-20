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
struct Entry {
    value: String,
    deadline: Option<u64>,
}

#[derive(Clone)]
struct Version {
    revision: u64,
    entry: Option<Entry>,
}

struct Transaction {
    begin_revision: u64,
    read_only: bool,
    writes: BTreeMap<String, Option<Entry>>,
}

pub struct Store {
    clock: u64,
    revision: u64,
    data: BTreeMap<String, Vec<Version>>,
    transactions: BTreeMap<u64, Transaction>,
    snapshots: BTreeMap<String, BTreeMap<String, Entry>>,
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

    fn transaction(&self, id: u64) -> Result<&Transaction, Error> {
        self.transactions
            .get(&id)
            .ok_or(Error::UnknownTransaction(id))
    }

    fn transaction_mut(&mut self, id: u64) -> Result<&mut Transaction, Error> {
        self.transactions
            .get_mut(&id)
            .ok_or(Error::UnknownTransaction(id))
    }

    fn selected_entry<'a>(
        &'a self,
        key: &str,
        tx: Option<u64>,
    ) -> Option<&'a Entry> {
        let revision = tx
            .and_then(|id| self.transactions.get(&id).map(|t| t.begin_revision));
        let versions = self.data.get(key)?;
        versions
            .iter()
            .rev()
            .find(|v| revision.map_or(true, |r| v.revision <= r))
            .and_then(|v| v.entry.as_ref())
    }

    fn visible_entry(&self, key: &str, tx: Option<u64>, now: u64) -> Option<&Entry> {
        if let Some(id) = tx {
            if let Some(entry) = self.transactions.get(&id)?.writes.get(key) {
                return entry.as_ref().filter(|e| e.deadline.map_or(true, |d| d > now));
            }
        }
        self.selected_entry(key, tx)
            .filter(|e| e.deadline.map_or(true, |d| d > now))
    }

    fn append(&mut self, key: String, entry: Option<Entry>) -> u64 {
        self.revision += 1;
        self.data.entry(key).or_default().push(Version {
            revision: self.revision,
            entry,
        });
        self.revision
    }

    fn stage_or_append(&mut self, tx: Option<u64>, key: String, entry: Option<Entry>) {
        if let Some(id) = tx {
            self.transactions
                .get_mut(&id)
                .expect("validated transaction")
                .writes
                .insert(key, entry);
        } else {
            self.append(key, entry);
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
    ) -> Vec<String> {
        let mut keys = BTreeSet::new();
        for key in self.data.keys() {
            keys.insert(key.as_str());
        }
        if let Some(id) = tx {
            if let Some(transaction) = self.transactions.get(&id) {
                for key in transaction.writes.keys() {
                    keys.insert(key.as_str());
                }
            }
        }

        keys.into_iter()
            .filter(|key| {
                *key >= start
                    && end.map_or(true, |bound| *key < bound)
                    && prefix.map_or(true, |p| key.starts_with(p))
                    && after.map_or(true, |last| *key > last)
                    && self.visible_entry(key, tx, now).is_some()
            })
            .map(str::to_owned)
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
        let transaction = self.transaction(id)?;
        if !transaction.writes.is_empty() {
            for key in transaction.writes.keys() {
                if self
                    .data
                    .get(key)
                    .and_then(|versions| versions.last())
                    .map_or(false, |v| v.revision > transaction.begin_revision)
                {
                    return Err(Error::Conflict {
                        tx: id,
                        key: key.clone(),
                    });
                }
            }
            let writes = self
                .transactions
                .remove(&id)
                .expect("validated transaction")
                .writes;
            self.revision += 1;
            let revision = self.revision;
            for (key, entry) in writes {
                self.data.entry(key).or_default().push(Version {
                    revision,
                    entry,
                });
            }
            Ok(revision)
        } else {
            self.transactions.remove(&id);
            Ok(self.revision)
        }
    }

    pub fn rollback(&mut self, now: u64, id: u64) -> Result<(), Error> {
        self.admit(now)?;
        self.transactions.remove(&id).ok_or(Error::UnknownTransaction(id))?;
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
            if key.is_empty() {
                return Err(Error::KeyEmpty);
            }
            if transaction.read_only {
                return Err(Error::ReadOnly(id));
            }
        } else if key.is_empty() {
            return Err(Error::KeyEmpty);
        }

        let entry = match ttl {
            Some(0) => None,
            Some(duration) => Some(Entry {
                value: value.to_owned(),
                deadline: Some(now.checked_add(duration).ok_or(Error::DeadlineOverflow)?),
            }),
            None => Some(Entry {
                value: value.to_owned(),
                deadline: None,
            }),
        };
        self.stage_or_append(tx, key.to_owned(), entry);
        Ok(())
    }

    pub fn delete(&mut self, now: u64, tx: Option<u64>, key: &str) -> Result<bool, Error> {
        self.admit(now)?;
        if let Some(id) = tx {
            let transaction = self.transaction(id)?;
            if key.is_empty() {
                return Err(Error::KeyEmpty);
            }
            if transaction.read_only {
                return Err(Error::ReadOnly(id));
            }
        } else if key.is_empty() {
            return Err(Error::KeyEmpty);
        }
        let existed = self.visible_entry(key, tx, now).is_some();
        self.stage_or_append(tx, key.to_owned(), None);
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
        if key.is_empty() {
            return Err(Error::KeyEmpty);
        }
        Ok(self
            .visible_entry(key, tx, now)
            .map(|entry| entry.value.clone()))
    }

    pub fn count(
        &mut self,
        now: u64,
        tx: Option<u64>,
        value: &str,
    ) -> Result<usize, Error> {
        self.admit(now)?;
        if let Some(id) = tx {
            self.transaction(id)?;
        }
        let keys = self.matching_keys(tx, now, "", None, None, None);
        Ok(keys
            .iter()
            .filter(|key| {
                self.visible_entry(key, tx, now)
                    .map_or(false, |entry| entry.value == value)
            })
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
        if end.map_or(false, |bound| bound <= start) {
            return Err(Error::InvalidRange);
        }
        if let Some(cursor) = cursor {
            if cursor.start != start
                || cursor.end.as_deref() != end
                || cursor.prefix.as_deref() != prefix
                || cursor.offset != offset
                || cursor.last.is_empty()
                || cursor.last.as_str() < start
                || end.map_or(false, |bound| cursor.last.as_str() >= bound)
                || prefix.map_or(false, |p| !cursor.last.starts_with(p))
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

        let keys = self.matching_keys(
            tx,
            now,
            start,
            end,
            prefix,
            cursor.map(|c| c.last.as_str()),
        );
        let skip = if cursor.is_some() { 0 } else { offset };
        let mut items = Vec::new();
        let mut has_more = false;
        for key in keys.into_iter().skip(skip) {
            if items.len() == limit {
                has_more = true;
                break;
            }
            if let Some(entry) = self.visible_entry(&key, tx, now) {
                items.push((key, entry.value.clone()));
            }
        }
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
        if end.map_or(false, |bound| bound <= start) {
            return Err(Error::InvalidRange);
        }
        let keys = self.matching_keys(tx, now, start, end, prefix, None);
        let mut count = 0usize;
        let mut sum = 0i64;
        for key in keys {
            let entry = self.visible_entry(&key, tx, now).expect("matching live key");
            let value = &entry.value;
            if !is_canonical_i64(value) {
                return Err(Error::InvalidNumber {
                    key,
                    value: value.clone(),
                });
            }
            let parsed = match value.parse::<i64>() {
                Ok(n) => n,
                Err(_) => {
                    return Err(Error::InvalidNumber {
                        key,
                        value: value.clone(),
                    });
                }
            };
            sum = sum.checked_add(parsed).ok_or_else(|| Error::SumOverflow {
                key: key.clone(),
            })?;
            count += 1;
        }
        Ok(Aggregate { count, sum })
    }

    pub fn snapshot(&mut self, now: u64, name: &str) -> Result<(), Error> {
        self.admit(now)?;
        validate_name(name)?;
        let exists = self.snapshots.contains_key(name);
        if !exists && self.snapshots.len() >= 3 {
            return Err(Error::SnapshotLimit);
        }
        let mut copy = BTreeMap::new();
        for (key, versions) in &self.data {
            if let Some(entry) = versions
                .last()
                .and_then(|version| version.entry.as_ref())
                .filter(|entry| entry.deadline.map_or(true, |deadline| deadline > now))
            {
                copy.insert(key.clone(), entry.clone());
            }
        }
        self.snapshots.insert(name.to_owned(), copy);
        Ok(())
    }

    pub fn restore(&mut self, now: u64, name: &str) -> Result<u64, Error> {
        self.admit(now)?;
        validate_name(name)?;
        let saved = self
            .snapshots
            .get(name)
            .ok_or_else(|| Error::SnapshotMissing(name.to_owned()))?;
        if !self.transactions.is_empty() {
            return Err(Error::ActiveTransactions);
        }

        let copied: BTreeMap<String, Entry> = saved
            .iter()
            .filter(|(_, entry)| entry.deadline.map_or(true, |deadline| deadline > now))
            .map(|(key, entry)| (key.clone(), entry.clone()))
            .collect();
        let mut union = BTreeSet::new();
        for (key, versions) in &self.data {
            if versions
                .last()
                .and_then(|version| version.entry.as_ref())
                .map_or(false, |entry| entry.deadline.map_or(true, |d| d > now))
            {
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
            let entry = copied.get(&key).cloned();
            self.data.entry(key).or_default().push(Version { revision, entry });
        }
        Ok(revision)
    }

    pub fn forget(&mut self, now: u64, name: &str) -> Result<(), Error> {
        self.admit(now)?;
        validate_name(name)?;
        self.snapshots
            .remove(name)
            .ok_or_else(|| Error::SnapshotMissing(name.to_owned()))?;
        Ok(())
    }

    pub fn gc(&mut self, now: u64) -> Result<usize, Error> {
        self.admit(now)?;
        let mut removed = 0usize;
        for versions in self.data.values_mut() {
            let mut keep = BTreeSet::new();
            if let Some(last) = versions.len().checked_sub(1) {
                keep.insert(last);
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
                let retained = keep.contains(&index);
                index += 1;
                if !retained {
                    removed += 1;
                }
                retained
            });
        }
        Ok(removed)
    }

    pub fn stats(&mut self, now: u64) -> Result<Stats, Error> {
        self.admit(now)?;
        let keys = self.data.len();
        let mut live_keys = 0usize;
        let mut versions = 0usize;
        let mut tombstones = 0usize;
        for history in self.data.values() {
            versions += history.len();
            tombstones += history.iter().filter(|version| version.entry.is_none()).count();
            if history
                .last()
                .and_then(|version| version.entry.as_ref())
                .map_or(false, |entry| entry.deadline.map_or(true, |d| d > now))
            {
                live_keys += 1;
            }
        }
        let read_only_transactions = self
            .transactions
            .values()
            .filter(|transaction| transaction.read_only)
            .count();
        let staged_writes = self
            .transactions
            .values()
            .map(|transaction| transaction.writes.len())
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

fn validate_name(name: &str) -> Result<(), Error> {
    if name.is_empty() {
        return Err(Error::NameEmpty);
    }
    let bytes = name.as_bytes();
    if !bytes[0].is_ascii_lowercase() {
        return Err(Error::NameInvalid { at: 0 });
    }
    for (index, byte) in bytes.iter().enumerate().skip(1) {
        if !(byte.is_ascii_lowercase() || byte.is_ascii_digit() || *byte == b'_') {
            return Err(Error::NameInvalid { at: index });
        }
    }
    Ok(())
}

fn is_canonical_i64(value: &str) -> bool {
    let bytes = value.as_bytes();
    if bytes.is_empty() {
        return false;
    }
    let digits = if bytes[0] == b'-' { &bytes[1..] } else { bytes };
    if digits.is_empty() || !digits.iter().all(u8::is_ascii_digit) {
        return false;
    }
    if digits == b"0" {
        return bytes[0] != b'-';
    }
    digits[0] != b'0'
}