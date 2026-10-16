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

#[derive(Clone)]
struct Transaction {
    begin_revision: u64,
    read_only: bool,
    writes: BTreeMap<String, Option<Entry>>,
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

    fn check_tx(&self, tx: Option<u64>) -> Result<(), Error> {
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
            if let Some(write) = transaction.writes.get(key) {
                return write.clone();
            }
            let versions = self.versions.get(key)?;
            versions
                .iter()
                .rev()
                .find(|version| version.revision <= transaction.begin_revision)
                .and_then(|version| version.entry.clone())
        } else {
            self.versions
                .get(key)?
                .last()
                .and_then(|version| version.entry.clone())
        }
    }

    fn visible_entries(&self, tx: Option<u64>) -> BTreeMap<String, Entry> {
        let mut keys: BTreeSet<String> = self.versions.keys().cloned().collect();
        if let Some(id) = tx {
            if let Some(transaction) = self.transactions.get(&id) {
                keys.extend(transaction.writes.keys().cloned());
            }
        }
        let mut result = BTreeMap::new();
        for key in keys {
            if let Some(entry) = self.selected_entry(&key, tx) {
                if entry.deadline.map_or(true, |deadline| deadline > self.clock) {
                    result.insert(key, entry);
                }
            }
        }
        result
    }

    fn append_version(&mut self, key: String, entry: Option<Entry>, revision: u64) {
        self.versions
            .entry(key)
            .or_default()
            .push(Version { revision, entry });
    }

    fn validate_key(key: &str) -> Result<(), Error> {
        if key.is_empty() {
            Err(Error::KeyEmpty)
        } else {
            Ok(())
        }
    }

    fn validate_range(start: &str, end: Option<&str>) -> Result<(), Error> {
        if let Some(end) = end {
            if end <= start {
                return Err(Error::InvalidRange);
            }
        }
        Ok(())
    }

    fn matches(key: &str, start: &str, end: Option<&str>, prefix: Option<&str>) -> bool {
        key >= start
            && end.map_or(true, |bound| key < bound)
            && prefix.map_or(true, |p| key.starts_with(p))
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
            .cloned()
            .ok_or(Error::UnknownTransaction(id))?;

        if !transaction.writes.is_empty() {
            for key in transaction.writes.keys() {
                if self
                    .versions
                    .get(key)
                    .and_then(|versions| versions.last())
                    .map_or(false, |head| head.revision > transaction.begin_revision)
                {
                    return Err(Error::Conflict {
                        tx: id,
                        key: key.clone(),
                    });
                }
            }

            self.revision += 1;
            let revision = self.revision;
            for (key, entry) in transaction.writes {
                self.append_version(key, entry, revision);
            }
            self.transactions.remove(&id);
            Ok(revision)
        } else {
            self.transactions.remove(&id);
            Ok(self.revision)
        }
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
        Self::validate_key(key)?;
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
            None
        } else {
            Some(Entry {
                value: value.to_string(),
                deadline,
            })
        };

        if let Some(id) = tx {
            self.transactions
                .get_mut(&id)
                .unwrap()
                .writes
                .insert(key.to_string(), entry);
        } else {
            self.revision += 1;
            self.append_version(key.to_string(), entry, self.revision);
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

        let existed = self
            .selected_entry(key, tx)
            .map_or(false, |entry| entry.deadline.map_or(true, |deadline| deadline > now));

        if let Some(id) = tx {
            self.transactions
                .get_mut(&id)
                .unwrap()
                .writes
                .insert(key.to_string(), None);
        } else {
            self.revision += 1;
            self.append_version(key.to_string(), None, self.revision);
        }
        Ok(existed)
    }

    pub fn get(&mut self, now: u64, tx: Option<u64>, key: &str) -> Result<Option<String>, Error> {
        self.admit(now)?;
        self.check_tx(tx)?;
        Self::validate_key(key)?;
        Ok(self.selected_entry(key, tx).and_then(|entry| {
            if entry.deadline.map_or(true, |deadline| deadline > now) {
                Some(entry.value)
            } else {
                None
            }
        }))
    }

    pub fn count(
        &mut self,
        now: u64,
        tx: Option<u64>,
        value: &str,
    ) -> Result<usize, Error> {
        self.admit(now)?;
        self.check_tx(tx)?;
        Ok(self
            .visible_entries(tx)
            .values()
            .filter(|entry| entry.value == value)
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
                || !Self::matches(&cursor.last, start, end, prefix)
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

        let after = cursor.map(|c| c.last.as_str());
        let entries = self.visible_entries(tx);
        let filtered: Vec<(String, String)> = entries
            .into_iter()
            .filter(|(key, _)| Self::matches(key, start, end, prefix))
            .filter(|(key, _)| after.map_or(true, |last| key.as_str() > last))
            .map(|(key, entry)| (key, entry.value))
            .collect();

        let skip = if cursor.is_some() { 0 } else { offset };
        let items: Vec<_> = filtered.iter().skip(skip).take(limit).cloned().collect();
        let has_more = filtered.len().saturating_sub(skip) > items.len();
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
        self.admit(now)?;
        self.check_tx(tx)?;
        Self::validate_range(start, end)?;

        let entries = self.visible_entries(tx);
        let mut count = 0usize;
        let mut sum = 0i64;
        for (key, entry) in entries {
            if !Self::matches(&key, start, end, prefix) {
                continue;
            }
            let value = parse_canonical_i64(&entry.value).ok_or_else(|| Error::InvalidNumber {
                key: key.clone(),
                value: entry.value.clone(),
            })?;
            sum = sum
                .checked_add(value)
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
        let entries = self.visible_entries(None);
        self.snapshots.insert(name.to_string(), entries);
        Ok(())
    }

    pub fn restore(&mut self, now: u64, name: &str) -> Result<u64, Error> {
        self.admit(now)?;
        Self::valid_name(name)?;
        let saved = self
            .snapshots
            .get(name)
            .cloned()
            .ok_or_else(|| Error::SnapshotMissing(name.to_string()))?;
        if !self.transactions.is_empty() {
            return Err(Error::ActiveTransactions);
        }

        let saved_live: BTreeMap<String, Entry> = saved
            .into_iter()
            .filter(|(_, entry)| entry.deadline.map_or(true, |deadline| deadline > now))
            .collect();
        let current_live = self.visible_entries(None);
        let mut union: BTreeSet<String> = current_live.keys().cloned().collect();
        union.extend(saved_live.keys().cloned());

        if union.is_empty() {
            return Ok(self.revision);
        }

        self.revision += 1;
        let revision = self.revision;
        for key in union {
            self.append_version(key.clone(), saved_live.get(&key).cloned(), revision);
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
        let mut removed = 0usize;

        for versions in self.versions.values_mut() {
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
        let versions = self.versions.values().map(Vec::len).sum();
        let tombstones = self
            .versions
            .values()
            .flat_map(|versions| versions.iter())
            .filter(|version| version.entry.is_none())
            .count();
        Ok(Stats {
            clock: self.clock,
            revision: self.revision,
            keys: self.versions.len(),
            live_keys: self.visible_entries(None).len(),
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