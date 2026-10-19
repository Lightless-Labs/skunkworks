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
enum Data {
    Value { value: String, deadline: Option<u64> },
    Tombstone,
}

#[derive(Clone)]
struct Version {
    revision: u64,
    data: Data,
}

#[derive(Clone)]
struct Transaction {
    begin_revision: u64,
    read_only: bool,
    writes: BTreeMap<String, Data>,
}

#[derive(Clone)]
struct SnapshotEntry {
    value: String,
    deadline: Option<u64>,
}

pub struct Store {
    clock: u64,
    revision: u64,
    versions: BTreeMap<String, Vec<Version>>,
    transactions: BTreeMap<u64, Transaction>,
    snapshots: BTreeMap<String, BTreeMap<String, SnapshotEntry>>,
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

    fn key_check(key: &str) -> Result<(), Error> {
        if key.is_empty() {
            Err(Error::KeyEmpty)
        } else {
            Ok(())
        }
    }

    fn name_check(name: &str) -> Result<(), Error> {
        if name.is_empty() {
            return Err(Error::NameEmpty);
        }
        let bytes = name.as_bytes();
        if !bytes[0].is_ascii_lowercase() {
            return Err(Error::NameInvalid { at: 0 });
        }
        for (i, b) in bytes.iter().enumerate().skip(1) {
            if !(b.is_ascii_lowercase() || b.is_ascii_digit() || *b == b'_') {
                return Err(Error::NameInvalid { at: i });
            }
        }
        Ok(())
    }

    fn selected_data(&self, key: &str, tx: Option<u64>) -> Option<Data> {
        if let Some(id) = tx {
            let transaction = self.transactions.get(&id)?;
            if let Some(write) = transaction.writes.get(key) {
                return Some(write.clone());
            }
            let history = self.versions.get(key)?;
            history
                .iter()
                .rev()
                .find(|v| v.revision <= transaction.begin_revision)
                .map(|v| v.data.clone())
        } else {
            self.versions
                .get(key)?
                .last()
                .map(|v| v.data.clone())
        }
    }

    fn live_value(data: Option<Data>, now: u64) -> Option<String> {
        match data {
            Some(Data::Value { value, deadline }) if deadline.map_or(true, |d| d > now) => {
                Some(value)
            }
            _ => None,
        }
    }

    fn visible_map(&self, tx: Option<u64>, now: u64) -> BTreeMap<String, String> {
        let mut keys: BTreeSet<String> = self.versions.keys().cloned().collect();
        if let Some(id) = tx {
            if let Some(t) = self.transactions.get(&id) {
                keys.extend(t.writes.keys().cloned());
            }
        }
        let mut result = BTreeMap::new();
        for key in keys {
            if let Some(value) = Self::live_value(self.selected_data(&key, tx), now) {
                result.insert(key, value);
            }
        }
        result
    }

    fn append_committed(&mut self, key: String, data: Data) {
        self.revision += 1;
        self.versions
            .entry(key)
            .or_default()
            .push(Version {
                revision: self.revision,
                data,
            });
    }

    fn range_check(start: &str, end: Option<&str>) -> Result<(), Error> {
        if end.is_some_and(|e| e <= start) {
            return Err(Error::InvalidRange);
        }
        Ok(())
    }

    fn matches_range(key: &str, start: &str, end: Option<&str>, prefix: Option<&str>) -> bool {
        key >= start
            && end.map_or(true, |e| key < e)
            && prefix.map_or(true, |p| key.starts_with(p))
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

        for key in transaction.writes.keys() {
            if self
                .versions
                .get(key)
                .and_then(|history| history.last())
                .is_some_and(|v| v.revision > transaction.begin_revision)
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

        let writes = transaction.writes.clone();
        self.revision += 1;
        let revision = self.revision;
        for (key, data) in writes {
            self.versions
                .entry(key)
                .or_default()
                .push(Version { revision, data });
        }
        self.transactions.remove(&id);
        Ok(revision)
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
        self.validate_tx(tx)?;
        Self::key_check(key)?;
        if let Some(id) = tx {
            if self.transactions.get(&id).unwrap().read_only {
                return Err(Error::ReadOnly(id));
            }
        }

        let data = match ttl {
            Some(0) => Data::Tombstone,
            Some(duration) => Data::Value {
                value: value.to_owned(),
                deadline: Some(now.checked_add(duration).ok_or(Error::DeadlineOverflow)?),
            },
            None => Data::Value {
                value: value.to_owned(),
                deadline: None,
            },
        };

        if let Some(id) = tx {
            self.transactions
                .get_mut(&id)
                .unwrap()
                .writes
                .insert(key.to_owned(), data);
        } else {
            self.append_committed(key.to_owned(), data);
        }
        Ok(())
    }

    pub fn delete(&mut self, now: u64, tx: Option<u64>, key: &str) -> Result<bool, Error> {
        self.admit(now)?;
        self.validate_tx(tx)?;
        Self::key_check(key)?;
        if let Some(id) = tx {
            if self.transactions.get(&id).unwrap().read_only {
                return Err(Error::ReadOnly(id));
            }
        }

        let existed = Self::live_value(self.selected_data(key, tx), now).is_some();
        if let Some(id) = tx {
            self.transactions
                .get_mut(&id)
                .unwrap()
                .writes
                .insert(key.to_owned(), Data::Tombstone);
        } else {
            self.append_committed(key.to_owned(), Data::Tombstone);
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
        self.validate_tx(tx)?;
        Self::key_check(key)?;
        Ok(Self::live_value(self.selected_data(key, tx), now))
    }

    pub fn count(
        &mut self,
        now: u64,
        tx: Option<u64>,
        value: &str,
    ) -> Result<usize, Error> {
        self.admit(now)?;
        self.validate_tx(tx)?;
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
        self.validate_tx(tx)?;
        Self::range_check(start, end)?;

        if let Some(c) = cursor {
            if c.start != start
                || c.end.as_deref() != end
                || c.prefix.as_deref() != prefix
                || c.offset != offset
                || c.last.is_empty()
                || !Self::matches_range(&c.last, start, end, prefix)
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

        let visible = self.visible_map(tx, now);
        let mut keys: Vec<String> = visible
            .keys()
            .filter(|key| Self::matches_range(key, start, end, prefix))
            .filter(|key| cursor.map_or(true, |c| key.as_str() > c.last.as_str()))
            .cloned()
            .collect();

        if cursor.is_none() {
            keys = keys.into_iter().skip(offset).collect();
        }

        let has_more = keys.len() > limit;
        keys.truncate(limit);
        let items: Vec<(String, String)> = keys
            .into_iter()
            .filter_map(|key| visible.get(&key).map(|value| (key, value.clone())))
            .collect();

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
        Self::range_check(start, end)?;

        let visible = self.visible_map(tx, now);
        let mut count = 0usize;
        let mut sum = 0i64;

        for (key, value) in visible {
            if !Self::matches_range(&key, start, end, prefix) {
                continue;
            }
            let parsed = Self::parse_canonical_i64(&value).ok_or_else(|| Error::InvalidNumber {
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

        if digits.is_empty() || !digits.iter().all(u8::is_ascii_digit) {
            return None;
        }
        if digits.len() > 1 && digits[0] == b'0' {
            return None;
        }
        if bytes[0] == b'-' && digits == b"0" {
            return None;
        }

        value.parse::<i64>().ok()
    }

    pub fn snapshot(&mut self, now: u64, name: &str) -> Result<(), Error> {
        self.admit(now)?;
        Self::name_check(name)?;

        if !self.snapshots.contains_key(name) && self.snapshots.len() >= 3 {
            return Err(Error::SnapshotLimit);
        }

        let mut saved = BTreeMap::new();
        for (key, history) in &self.versions {
            if let Some(Version {
                data: Data::Value { value, deadline },
                ..
            }) = history.last()
            {
                if deadline.map_or(true, |d| d > now) {
                    saved.insert(
                        key.clone(),
                        SnapshotEntry {
                            value: value.clone(),
                            deadline: *deadline,
                        },
                    );
                }
            }
        }
        self.snapshots.insert(name.to_owned(), saved);
        Ok(())
    }

    pub fn restore(&mut self, now: u64, name: &str) -> Result<u64, Error> {
        self.admit(now)?;
        Self::name_check(name)?;

        let saved = self
            .snapshots
            .get(name)
            .cloned()
            .ok_or_else(|| Error::SnapshotMissing(name.to_owned()))?;

        if !self.transactions.is_empty() {
            return Err(Error::ActiveTransactions);
        }

        let copied: BTreeMap<String, SnapshotEntry> = saved
            .into_iter()
            .filter(|(_, entry)| entry.deadline.map_or(true, |d| d > now))
            .collect();

        let current = self.visible_map(None, now);
        let keys: BTreeSet<String> = current.keys().chain(copied.keys()).cloned().collect();
        if keys.is_empty() {
            return Ok(self.revision);
        }

        self.revision += 1;
        let revision = self.revision;
        for key in keys {
            let data = match copied.get(&key) {
                Some(entry) => Data::Value {
                    value: entry.value.clone(),
                    deadline: entry.deadline,
                },
                None => Data::Tombstone,
            };
            self.versions
                .entry(key)
                .or_default()
                .push(Version { revision, data });
        }
        Ok(revision)
    }

    pub fn forget(&mut self, now: u64, name: &str) -> Result<(), Error> {
        self.admit(now)?;
        Self::name_check(name)?;
        if self.snapshots.remove(name).is_none() {
            return Err(Error::SnapshotMissing(name.to_owned()));
        }
        Ok(())
    }

    pub fn gc(&mut self, now: u64) -> Result<usize, Error> {
        self.admit(now)?;
        let mut removed = 0usize;

        for history in self.versions.values_mut() {
            if history.len() <= 1 {
                continue;
            }

            let mut keep: BTreeSet<usize> = BTreeSet::new();
            keep.insert(history.len() - 1);

            for transaction in self.transactions.values() {
                if let Some(index) = history
                    .iter()
                    .rposition(|v| v.revision <= transaction.begin_revision)
                {
                    keep.insert(index);
                }
            }

            let mut index = 0usize;
            history.retain(|_| {
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
        let live_keys = self.visible_map(None, now).len();
        let versions = self.versions.values().map(Vec::len).sum();
        let tombstones = self
            .versions
            .values()
            .flat_map(|history| history.iter())
            .filter(|v| matches!(v.data, Data::Tombstone))
            .count();
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