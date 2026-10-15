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
    versions: BTreeMap<String, Vec<Version>>,
    transactions: BTreeMap<u64, Transaction>,
    snapshots: BTreeMap<String, Snapshot>,
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

    fn transaction(&self, tx: u64) -> Result<&Transaction, Error> {
        self.transactions.get(&tx).ok_or(Error::UnknownTransaction(tx))
    }

    fn transaction_mut(&mut self, tx: u64) -> Result<&mut Transaction, Error> {
        self.transactions.get_mut(&tx).ok_or(Error::UnknownTransaction(tx))
    }

    fn selected_version(&self, key: &str, tx: Option<u64>) -> Option<&Version> {
        let history = self.versions.get(key)?;
        match tx {
            None => history.last(),
            Some(id) => {
                let begin_revision = self.transactions.get(&id)?.begin_revision;
                history
                    .iter()
                    .rev()
                    .find(|version| version.revision <= begin_revision)
            }
        }
    }

    fn visible_value(&self, key: &str, tx: Option<u64>, now: u64) -> Option<String> {
        if let Some(id) = tx {
            if let Some(write) = self.transactions.get(&id)?.writes.get(key) {
                return Self::live_value(&write.value, write.deadline, now);
            }
        }
        let version = self.selected_version(key, tx)?;
        Self::live_value(&version.value, version.deadline, now)
    }

    fn live_value(value: &Option<String>, deadline: Option<u64>, now: u64) -> Option<String> {
        if deadline.is_some_and(|d| d <= now) {
            return None;
        }
        value.clone()
    }

    fn visible_entries(&self, tx: Option<u64>, now: u64) -> BTreeMap<String, String> {
        let mut keys: BTreeSet<String> = self.versions.keys().cloned().collect();
        if let Some(id) = tx {
            if let Some(transaction) = self.transactions.get(&id) {
                keys.extend(transaction.writes.keys().cloned());
            }
        }
        let mut entries = BTreeMap::new();
        for key in keys {
            if let Some(value) = self.visible_value(&key, tx, now) {
                entries.insert(key, value);
            }
        }
        entries
    }

    fn matches(key: &str, start: &str, end: Option<&str>, prefix: Option<&str>) -> bool {
        key >= start
            && end.is_none_or(|bound| key < bound)
            && prefix.is_none_or(|p| key.starts_with(p))
    }

    fn validate_name(name: &str) -> Result<(), Error> {
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

    fn append_version(&mut self, key: String, write: Write, revision: u64) {
        self.versions.entry(key).or_default().push(Version {
            revision,
            value: write.value,
            deadline: write.deadline,
        });
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
                .and_then(|history| history.last())
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

        let writes = self.transactions.remove(&id).unwrap().writes;
        self.revision += 1;
        let revision = self.revision;
        for (key, write) in writes {
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
            self.transaction(id)?;
        }
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
                self.transaction_mut(id)?
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
            self.transaction(id)?;
        }
        if key.is_empty() {
            return Err(Error::KeyEmpty);
        }
        if let Some(id) = tx {
            if self.transactions.get(&id).unwrap().read_only {
                return Err(Error::ReadOnly(id));
            }
        }

        let was_live = self.visible_value(key, tx, now).is_some();
        let write = Write {
            value: None,
            deadline: None,
        };
        match tx {
            Some(id) => {
                self.transaction_mut(id)?
                    .writes
                    .insert(key.to_owned(), write);
            }
            None => {
                self.revision += 1;
                self.append_version(key.to_owned(), write, self.revision);
            }
        }
        Ok(was_live)
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
        Ok(self.visible_value(key, tx, now))
    }

    pub fn count(&mut self, now: u64, tx: Option<u64>, value: &str) -> Result<usize, Error> {
        self.admit(now)?;
        if let Some(id) = tx {
            self.transaction(id)?;
        }
        Ok(self
            .visible_entries(tx, now)
            .values()
            .filter(|entry| entry.as_str() == value)
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
        if end.is_some_and(|bound| bound <= start) {
            return Err(Error::InvalidRange);
        }
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

        let entries = self.visible_entries(tx, now);
        let mut matching = entries
            .into_iter()
            .filter(|(key, _)| Self::matches(key, start, end, prefix))
            .filter(|(key, _)| cursor.is_none_or(|c| key > &c.last));

        if cursor.is_none() {
            for _ in 0..offset {
                if matching.next().is_none() {
                    return Ok(Page {
                        items: Vec::new(),
                        next: None,
                    });
                }
            }
        }

        let mut items = Vec::new();
        for _ in 0..limit {
            if let Some(item) = matching.next() {
                items.push(item);
            } else {
                break;
            }
        }

        let next = if !items.is_empty() && matching.next().is_some() {
            Some(Cursor {
                start: start.to_owned(),
                end: end.map(str::to_owned),
                prefix: prefix.map(str::to_owned),
                offset,
                last: items.last().unwrap().0.clone(),
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
        if end.is_some_and(|bound| bound <= start) {
            return Err(Error::InvalidRange);
        }

        let entries = self.visible_entries(tx, now);
        let mut count = 0usize;
        let mut sum = 0i64;
        for (key, value) in entries {
            if !Self::matches(&key, start, end, prefix) {
                continue;
            }
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
        if name.is_empty() {
            return Err(Error::NameEmpty);
        }
        Self::validate_name(name)?;
        if !self.snapshots.contains_key(name) && self.snapshots.len() >= 3 {
            return Err(Error::SnapshotLimit);
        }

        let entries = self
            .visible_entries(None, now)
            .into_iter()
            .filter_map(|(key, value)| {
                let version = self.versions.get(&key)?.last()?;
                Some((key, (value, version.deadline)))
            })
            .collect();
        self.snapshots
            .insert(name.to_owned(), Snapshot { entries });
        Ok(())
    }

    pub fn restore(&mut self, now: u64, name: &str) -> Result<u64, Error> {
        self.admit(now)?;
        if name.is_empty() {
            return Err(Error::NameEmpty);
        }
        Self::validate_name(name)?;
        let snapshot = self
            .snapshots
            .get(name)
            .cloned()
            .ok_or_else(|| Error::SnapshotMissing(name.to_owned()))?;
        if !self.transactions.is_empty() {
            return Err(Error::ActiveTransactions);
        }

        let copied: BTreeMap<String, (String, Option<u64>)> = snapshot
            .entries
            .into_iter()
            .filter(|(_, (_, deadline))| deadline.is_none_or(|d| d > now))
            .collect();
        let current = self.visible_entries(None, now);
        let mut union: BTreeSet<String> = current.keys().cloned().collect();
        union.extend(copied.keys().cloned());

        if union.is_empty() {
            return Ok(self.revision);
        }

        self.revision += 1;
        let revision = self.revision;
        for key in union {
            let write = if let Some((value, deadline)) = copied.get(&key) {
                Write {
                    value: Some(value.clone()),
                    deadline: *deadline,
                }
            } else {
                Write {
                    value: None,
                    deadline: None,
                }
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
        self.snapshots
            .remove(name)
            .ok_or_else(|| Error::SnapshotMissing(name.to_owned()))?;
        Ok(())
    }

    pub fn gc(&mut self, now: u64) -> Result<usize, Error> {
        self.admit(now)?;
        let mut removed = 0usize;
        for history in self.versions.values_mut() {
            if history.len() <= 1 {
                continue;
            }

            let mut retain = BTreeSet::new();
            retain.insert(history.len() - 1);

            for transaction in self.transactions.values() {
                if let Some(index) = history
                    .iter()
                    .rposition(|version| version.revision <= transaction.begin_revision)
                {
                    retain.insert(index);
                }
            }

            let mut index = 0usize;
            history.retain(|_| {
                let keep = retain.contains(&index);
                index += 1;
                if !keep {
                    removed += 1;
                }
                keep
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
            .flat_map(|history| history.iter())
            .filter(|version| version.value.is_none())
            .count();
        let live_keys = self.visible_entries(None, now).len();
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
            keys: self.versions.len(),
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

    if digits.iter().any(|byte| !byte.is_ascii_digit()) {
        return None;
    }
    if digits.len() > 1 && digits[0] == b'0' {
        return None;
    }
    if bytes[0] == b'-' && digits == b"0" {
        return None;
    }
    if bytes[0] == b'0' && bytes.len() != 1 {
        return None;
    }

    value.parse::<i64>().ok()
}

fn main() {}