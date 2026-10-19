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
    data: BTreeMap<String, Vec<Version>>,
    transactions: HashMap<u64, Transaction>,
    snapshots: BTreeMap<String, BTreeMap<String, Entry>>,
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

    fn ensure_key(key: &str) -> Result<(), Error> {
        if key.is_empty() {
            Err(Error::KeyEmpty)
        } else {
            Ok(())
        }
    }

    fn visible_entry(&self, tx: Option<u64>, key: &str) -> Option<Entry> {
        if let Some(id) = tx {
            let transaction = self.transactions.get(&id)?;
            if let Some(entry) = transaction.writes.get(key) {
                return entry.clone();
            }
        }

        let versions = self.data.get(key)?;
        if let Some(id) = tx {
            let begin_revision = self.transactions.get(&id)?.begin_revision;
            versions
                .iter()
                .rev()
                .find(|version| version.revision <= begin_revision)
                .and_then(|version| version.entry.clone())
        } else {
            versions.last().and_then(|version| version.entry.clone())
        }
    }

    fn is_live(entry: &Entry, now: u64) -> bool {
        entry.deadline.is_none_or(|deadline| deadline > now)
    }

    fn visible_live_entries(
        &self,
        tx: Option<u64>,
        now: u64,
        start: &str,
        end: Option<&str>,
        prefix: Option<&str>,
    ) -> Vec<(String, String)> {
        let mut keys: BTreeSet<String> = self.data.keys().cloned().collect();
        if let Some(id) = tx {
            if let Some(transaction) = self.transactions.get(&id) {
                keys.extend(transaction.writes.keys().cloned());
            }
        }

        keys.into_iter()
            .filter(|key| key.as_str() >= start)
            .filter(|key| end.is_none_or(|end| key.as_str() < end))
            .filter(|key| prefix.is_none_or(|prefix| key.starts_with(prefix)))
            .filter_map(|key| {
                let entry = self.visible_entry(tx, &key)?;
                if Self::is_live(&entry, now) {
                    Some((key, entry.value))
                } else {
                    None
                }
            })
            .collect()
    }

    fn append_version(&mut self, key: String, entry: Option<Entry>, revision: u64) {
        self.data
            .entry(key)
            .or_default()
            .push(Version { revision, entry });
    }

    fn put_write(&mut self, tx: Option<u64>, key: String, entry: Option<Entry>) {
        if let Some(id) = tx {
            self.transactions
                .get_mut(&id)
                .expect("transaction validated")
                .writes
                .insert(key, entry);
        } else {
            self.revision += 1;
            self.append_version(key, entry, self.revision);
        }
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
        let transaction = self
            .transactions
            .get(&id)
            .cloned()
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
        self.admit_time(now)?;
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
        self.admit_time(now)?;
        self.check_tx(tx)?;
        Self::ensure_key(key)?;
        if let Some(id) = tx {
            if self.transactions.get(&id).unwrap().read_only {
                return Err(Error::ReadOnly(id));
            }
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
        self.put_write(tx, key.to_owned(), entry);
        Ok(())
    }

    pub fn delete(&mut self, now: u64, tx: Option<u64>, key: &str) -> Result<bool, Error> {
        self.admit_time(now)?;
        self.check_tx(tx)?;
        Self::ensure_key(key)?;
        if let Some(id) = tx {
            if self.transactions.get(&id).unwrap().read_only {
                return Err(Error::ReadOnly(id));
            }
        }

        let was_live = self
            .visible_entry(tx, key)
            .is_some_and(|entry| Self::is_live(&entry, now));
        self.put_write(tx, key.to_owned(), None);
        Ok(was_live)
    }

    pub fn get(
        &mut self,
        now: u64,
        tx: Option<u64>,
        key: &str,
    ) -> Result<Option<String>, Error> {
        self.admit_time(now)?;
        self.check_tx(tx)?;
        Self::ensure_key(key)?;
        Ok(self
            .visible_entry(tx, key)
            .filter(|entry| Self::is_live(entry, now))
            .map(|entry| entry.value))
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
            .visible_live_entries(tx, now, "", None, None)
            .iter()
            .filter(|(_, current)| current == value)
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
        self.admit_time(now)?;
        self.check_tx(tx)?;
        if end.is_some_and(|end| end <= start) {
            return Err(Error::InvalidRange);
        }
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

        let mut entries = self.visible_live_entries(tx, now, start, end, prefix);
        if let Some(cursor) = cursor {
            entries.retain(|(key, _)| key > &cursor.last);
        } else {
            entries = entries.into_iter().skip(offset).collect();
        }

        let take = limit.min(entries.len());
        let has_more = entries.len() > take;
        let items: Vec<(String, String)> = entries.into_iter().take(take).collect();
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
        self.admit_time(now)?;
        self.check_tx(tx)?;
        if end.is_some_and(|end| end <= start) {
            return Err(Error::InvalidRange);
        }

        let entries = self.visible_live_entries(tx, now, start, end, prefix);
        let mut count = 0usize;
        let mut sum = 0i64;
        for (key, value) in entries {
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
        self.admit_time(now)?;
        Self::validate_snapshot_name(name)?;
        let mut snapshot = BTreeMap::new();
        for (key, versions) in &self.data {
            if let Some(entry) = versions.last().and_then(|version| version.entry.as_ref()) {
                if Self::is_live(entry, now) {
                    snapshot.insert(key.clone(), entry.clone());
                }
            }
        }
        if !self.snapshots.contains_key(name) && self.snapshots.len() >= 3 {
            return Err(Error::SnapshotLimit);
        }
        self.snapshots.insert(name.to_owned(), snapshot);
        Ok(())
    }

    pub fn restore(&mut self, now: u64, name: &str) -> Result<u64, Error> {
        self.admit_time(now)?;
        Self::validate_snapshot_name(name)?;
        let saved = self
            .snapshots
            .get(name)
            .cloned()
            .ok_or_else(|| Error::SnapshotMissing(name.to_owned()))?;
        if !self.transactions.is_empty() {
            return Err(Error::ActiveTransactions);
        }

        let saved_live: BTreeMap<String, Entry> = saved
            .into_iter()
            .filter(|(_, entry)| Self::is_live(entry, now))
            .collect();
        let current_live: BTreeMap<String, Entry> = self
            .data
            .iter()
            .filter_map(|(key, versions)| {
                let entry = versions.last()?.entry.as_ref()?;
                if Self::is_live(entry, now) {
                    Some((key.clone(), entry.clone()))
                } else {
                    None
                }
            })
            .collect();

        let keys: BTreeSet<String> = current_live
            .keys()
            .chain(saved_live.keys())
            .cloned()
            .collect();
        if keys.is_empty() {
            return Ok(self.revision);
        }

        self.revision += 1;
        let revision = self.revision;
        for key in keys {
            self.append_version(key.clone(), saved_live.get(&key).cloned(), revision);
        }
        Ok(revision)
    }

    pub fn forget(&mut self, now: u64, name: &str) -> Result<(), Error> {
        self.admit_time(now)?;
        Self::validate_snapshot_name(name)?;
        if self.snapshots.remove(name).is_none() {
            return Err(Error::SnapshotMissing(name.to_owned()));
        }
        Ok(())
    }

    pub fn gc(&mut self, now: u64) -> Result<usize, Error> {
        self.admit_time(now)?;
        let begin_revisions: Vec<u64> = self
            .transactions
            .values()
            .map(|transaction| transaction.begin_revision)
            .collect();
        let mut removed = 0usize;

        for versions in self.data.values_mut() {
            if versions.is_empty() {
                continue;
            }
            let mut keep = BTreeSet::new();
            keep.insert(versions.len() - 1);

            for begin_revision in &begin_revisions {
                if let Some(index) = versions
                    .iter()
                    .rposition(|version| version.revision <= *begin_revision)
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
        let mut versions = 0usize;
        let mut tombstones = 0usize;
        let mut live_keys = 0usize;

        for history in self.data.values() {
            versions += history.len();
            tombstones += history.iter().filter(|version| version.entry.is_none()).count();
            if history
                .last()
                .and_then(|version| version.entry.as_ref())
                .is_some_and(|entry| Self::is_live(entry, now))
            {
                live_keys += 1;
            }
        }

        Ok(Stats {
            clock: self.clock,
            revision: self.revision,
            keys: self.data.values().filter(|history| !history.is_empty()).count(),
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

    fn validate_snapshot_name(name: &str) -> Result<(), Error> {
        if name.is_empty() {
            return Err(Error::NameEmpty);
        }
        for (index, byte) in name.bytes().enumerate() {
            let valid = if index == 0 {
                byte.is_ascii_lowercase()
            } else {
                byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'_'
            };
            if !valid {
                return Err(Error::NameInvalid { at: index });
            }
        }
        Ok(())
    }
}

fn parse_canonical_i64(value: &str) -> Option<i64> {
    if value == "0" {
        return Some(0);
    }
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
        bytes
    };

    if digits[0] == b'0' || !digits.iter().all(u8::is_ascii_digit) {
        return None;
    }
    value.parse::<i64>().ok()
}