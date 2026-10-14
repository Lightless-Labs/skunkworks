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
    value: Option<String>,
    deadline: Option<u64>,
}

#[derive(Clone)]
struct Version {
    revision: u64,
    entry: Entry,
}

struct Transaction {
    begin_revision: u64,
    read_only: bool,
    writes: BTreeMap<String, Entry>,
}

pub struct Store {
    clock: u64,
    revision: u64,
    versions: BTreeMap<String, Vec<Version>>,
    transactions: BTreeMap<u64, Transaction>,
    snapshots: BTreeMap<String, BTreeMap<String, Entry>>,
}

impl Store {
    pub fn new() -> Self {
        Self {
            clock: 0,
            revision: 0,
            versions: BTreeMap::new(),
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

    fn validate_tx(&self, tx: Option<u64>) -> Result<(), Error> {
        if let Some(id) = tx {
            if !self.transactions.contains_key(&id) {
                return Err(Error::UnknownTransaction(id));
            }
        }
        Ok(())
    }

    fn selected_entry(&self, key: &str, tx: Option<u64>) -> Option<Entry> {
        if let Some(id) = tx {
            let transaction = self.transactions.get(&id)?;
            if let Some(entry) = transaction.writes.get(key) {
                return Some(entry.clone());
            }
            self.versions.get(key)?.iter()
                .rev()
                .find(|version| version.revision <= transaction.begin_revision)
                .map(|version| version.entry.clone())
        } else {
            self.versions.get(key)?.last().map(|version| version.entry.clone())
        }
    }

    fn is_live(entry: &Entry, now: u64) -> bool {
        entry.value.is_some() && entry.deadline.map_or(true, |deadline| deadline > now)
    }

    fn live_view(&self, now: u64, tx: Option<u64>) -> BTreeMap<String, String> {
        let mut keys: BTreeSet<String> = self.versions.keys().cloned().collect();
        if let Some(id) = tx {
            if let Some(transaction) = self.transactions.get(&id) {
                keys.extend(transaction.writes.keys().cloned());
            }
        }

        let mut result = BTreeMap::new();
        for key in keys {
            if let Some(entry) = self.selected_entry(&key, tx) {
                if Self::is_live(&entry, now) {
                    if let Some(value) = entry.value {
                        result.insert(key, value);
                    }
                }
            }
        }
        result
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
        if end.is_some_and(|bound| bound <= start) {
            return Err(Error::InvalidRange);
        }
        Ok(())
    }

    fn matches_filters(key: &str, start: &str, end: Option<&str>, prefix: Option<&str>) -> bool {
        key >= start
            && end.map_or(true, |bound| key < bound)
            && prefix.map_or(true, |p| key.starts_with(p))
    }

    fn append_version(&mut self, key: String, entry: Entry, revision: u64) {
        self.versions.entry(key).or_default().push(Version { revision, entry });
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
        let transaction = self.transactions.get(&id).ok_or(Error::UnknownTransaction(id))?;

        for key in transaction.writes.keys() {
            if self.versions.get(key)
                .and_then(|list| list.last())
                .is_some_and(|version| version.revision > transaction.begin_revision)
            {
                return Err(Error::Conflict {
                    tx: id,
                    key: key.clone(),
                });
            }
        }

        if transaction.writes.is_empty() {
            self.transactions.remove(&id);
            return Ok(self.revision);
        }

        self.revision += 1;
        let revision = self.revision;
        let writes = self.transactions.remove(&id).unwrap().writes;
        for (key, entry) in writes {
            self.append_version(key, entry, revision);
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
        self.validate_tx(tx)?;
        if key.is_empty() {
            return Err(Error::KeyEmpty);
        }
        if let Some(id) = tx {
            if self.transactions.get(&id).unwrap().read_only {
                return Err(Error::ReadOnly(id));
            }
        }

        let deadline = match ttl {
            Some(duration) => Some(now.checked_add(duration).ok_or(Error::DeadlineOverflow)?),
            None => None,
        };

        let entry = if deadline == Some(now) {
            Entry {
                value: None,
                deadline: None,
            }
        } else {
            Entry {
                value: Some(value.to_owned()),
                deadline,
            }
        };

        if let Some(id) = tx {
            self.transactions.get_mut(&id).unwrap().writes.insert(key.to_owned(), entry);
        } else {
            self.revision += 1;
            self.append_version(key.to_owned(), entry, self.revision);
        }
        Ok(())
    }

    pub fn delete(&mut self, now: u64, tx: Option<u64>, key: &str) -> Result<bool, Error> {
        self.admit(now)?;
        self.validate_tx(tx)?;
        if key.is_empty() {
            return Err(Error::KeyEmpty);
        }
        if let Some(id) = tx {
            if self.transactions.get(&id).unwrap().read_only {
                return Err(Error::ReadOnly(id));
            }
        }

        let existed = self.selected_entry(key, tx)
            .is_some_and(|entry| Self::is_live(&entry, now));

        let tombstone = Entry {
            value: None,
            deadline: None,
        };

        if let Some(id) = tx {
            self.transactions.get_mut(&id).unwrap().writes.insert(key.to_owned(), tombstone);
        } else {
            self.revision += 1;
            self.append_version(key.to_owned(), tombstone, self.revision);
        }
        Ok(existed)
    }

    pub fn get(&mut self, now: u64, tx: Option<u64>, key: &str) -> Result<Option<String>, Error> {
        self.admit(now)?;
        self.validate_tx(tx)?;
        if key.is_empty() {
            return Err(Error::KeyEmpty);
        }

        Ok(self.selected_entry(key, tx).and_then(|entry| {
            if Self::is_live(&entry, now) {
                entry.value
            } else {
                None
            }
        }))
    }

    pub fn count(&mut self, now: u64, tx: Option<u64>, value: &str) -> Result<usize, Error> {
        self.admit(now)?;
        self.validate_tx(tx)?;
        Ok(self.live_view(now, tx).values().filter(|v| v.as_str() == value).count())
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
        self.validate_tx(tx)?;
        Self::validate_range(start, end)?;

        if let Some(cursor) = cursor {
            if cursor.start != start
                || cursor.end.as_deref() != end
                || cursor.prefix.as_deref() != prefix
                || cursor.offset != offset
                || cursor.last.is_empty()
                || !Self::matches_filters(&cursor.last, start, end, prefix)
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

        let view = self.live_view(now, tx);
        let mut matching: Vec<(String, String)> = view
            .into_iter()
            .filter(|(key, _)| Self::matches_filters(key, start, end, prefix))
            .filter(|(key, _)| cursor.map_or(true, |c| key.as_str() > c.last.as_str()))
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
        self.validate_tx(tx)?;
        Self::validate_range(start, end)?;

        let view = self.live_view(now, tx);
        let mut count = 0usize;
        let mut sum = 0i64;

        for (key, value) in view {
            if !Self::matches_filters(&key, start, end, prefix) {
                continue;
            }
            let parsed = parse_canonical_i64(&value).ok_or_else(|| Error::InvalidNumber {
                key: key.clone(),
                value: value.clone(),
            })?;
            sum = sum.checked_add(parsed).ok_or_else(|| Error::SumOverflow {
                key: key.clone(),
            })?;
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

        let mut saved = BTreeMap::new();
        for (key, versions) in &self.versions {
            if let Some(entry) = versions.last().map(|version| &version.entry) {
                if Self::is_live(entry, now) {
                    saved.insert(key.clone(), entry.clone());
                }
            }
        }
        self.snapshots.insert(name.to_owned(), saved);
        Ok(())
    }

    pub fn restore(&mut self, now: u64, name: &str) -> Result<u64, Error> {
        self.admit(now)?;
        if name.is_empty() {
            return Err(Error::NameEmpty);
        }
        let saved = self.snapshots.get(name)
            .cloned()
            .ok_or_else(|| Error::SnapshotMissing(name.to_owned()))?;

        if !self.transactions.is_empty() {
            return Err(Error::ActiveTransactions);
        }

        let live_saved: BTreeMap<String, Entry> = saved
            .into_iter()
            .filter(|(_, entry)| Self::is_live(entry, now))
            .collect();

        let live_current: BTreeSet<String> = self.versions
            .iter()
            .filter_map(|(key, versions)| {
                versions.last()
                    .filter(|version| Self::is_live(&version.entry, now))
                    .map(|_| key.clone())
            })
            .collect();

        let union: BTreeSet<String> = live_current
            .union(&live_saved.keys().cloned().collect())
            .cloned()
            .collect();

        if union.is_empty() {
            return Ok(self.revision);
        }

        self.revision += 1;
        let revision = self.revision;
        for key in union {
            let entry = live_saved.get(&key).cloned().unwrap_or(Entry {
                value: None,
                deadline: None,
            });
            self.append_version(key, entry, revision);
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
        let mut removed = 0usize;

        for versions in self.versions.values_mut() {
            if versions.is_empty() {
                continue;
            }

            let mut keep = BTreeSet::new();
            keep.insert(versions.len() - 1);

            for transaction in self.transactions.values() {
                if let Some(index) = versions.iter().rposition(|version| {
                    version.revision <= transaction.begin_revision
                }) {
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

        let keys = self.versions.len();
        let versions = self.versions.values().map(Vec::len).sum();
        let tombstones = self.versions.values()
            .flat_map(|versions| versions.iter())
            .filter(|version| version.entry.value.is_none())
            .count();
        let live_keys = self.versions.iter()
            .filter(|(_, versions)| versions.last()
                .is_some_and(|version| Self::is_live(&version.entry, now)))
            .count();
        let read_only_transactions = self.transactions.values()
            .filter(|transaction| transaction.read_only)
            .count();
        let staged_writes = self.transactions.values()
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

fn parse_canonical_i64(value: &str) -> Option<i64> {
    if value.is_empty() {
        return None;
    }

    let bytes = value.as_bytes();
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
    if digits[0] == b'0' && digits.len() > 1 {
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