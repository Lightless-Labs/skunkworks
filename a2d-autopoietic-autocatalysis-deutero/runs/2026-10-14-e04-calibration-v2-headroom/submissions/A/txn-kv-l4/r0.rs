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
            if let Some(entry) = transaction.writes.get(key) {
                return entry.clone();
            }
            let versions = self.data.get(key)?;
            versions
                .iter()
                .rev()
                .find(|v| v.revision <= transaction.begin_revision)
                .and_then(|v| v.entry.clone())
        } else {
            self.data
                .get(key)?
                .last()
                .and_then(|v| v.entry.clone())
        }
    }

    fn is_live(entry: &Entry, now: u64) -> bool {
        entry.deadline.map_or(true, |deadline| deadline > now)
    }

    fn visible(&self, tx: Option<u64>, now: u64) -> BTreeMap<String, String> {
        let mut result = BTreeMap::new();
        for key in self.data.keys() {
            if let Some(entry) = self.selected_entry(key, tx) {
                if Self::is_live(&entry, now) {
                    result.insert(key.clone(), entry.value);
                }
            }
        }
        if let Some(id) = tx {
            if let Some(transaction) = self.transactions.get(&id) {
                for (key, entry) in &transaction.writes {
                    if !self.data.contains_key(key) {
                        if let Some(entry) = entry {
                            if Self::is_live(entry, now) {
                                result.insert(key.clone(), entry.value.clone());
                            }
                        }
                    }
                }
            }
        }
        result
    }

    fn append_version(&mut self, key: String, entry: Option<Entry>, revision: u64) {
        self.data
            .entry(key)
            .or_default()
            .push(Version { revision, entry });
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
                let newer = self
                    .data
                    .get(key)
                    .and_then(|versions| versions.last())
                    .is_some_and(|version| version.revision > transaction.begin_revision);
                if newer {
                    return Err(Error::Conflict {
                        tx: id,
                        key: key.clone(),
                    });
                }
            }

            let writes = transaction.writes.clone();
            self.revision += 1;
            let revision = self.revision;
            for (key, entry) in writes {
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

        let entry = if ttl == Some(0) {
            None
        } else {
            let deadline = match ttl {
                Some(duration) => Some(now.checked_add(duration).ok_or(Error::DeadlineOverflow)?),
                None => None,
            };
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
        if key.is_empty() {
            return Err(Error::KeyEmpty);
        }
        if let Some(id) = tx {
            if self.transactions.get(&id).unwrap().read_only {
                return Err(Error::ReadOnly(id));
            }
        }

        let existed = self
            .selected_entry(key, tx)
            .is_some_and(|entry| Self::is_live(&entry, now));

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
        Ok(self
            .selected_entry(key, tx)
            .filter(|entry| Self::is_live(entry, now))
            .map(|entry| entry.value))
    }

    pub fn count(&mut self, now: u64, tx: Option<u64>, value: &str) -> Result<usize, Error> {
        self.admit(now)?;
        self.check_tx(tx)?;
        Ok(self
            .visible(tx, now)
            .values()
            .filter(|stored| stored.as_str() == value)
            .count())
    }

    fn validate_range(start: &str, end: Option<&str>) -> Result<(), Error> {
        if end.is_some_and(|end| end <= start) {
            return Err(Error::InvalidRange);
        }
        Ok(())
    }

    fn matches(key: &str, start: &str, end: Option<&str>, prefix: Option<&str>) -> bool {
        key >= start
            && end.map_or(true, |bound| key < bound)
            && prefix.map_or(true, |prefix| key.starts_with(prefix))
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

        let visible = self.visible(tx, now);
        let matching: Vec<(String, String)> = visible
            .into_iter()
            .filter(|(key, _)| {
                Self::matches(key, start, end, prefix)
                    && cursor.map_or(true, |cursor| key.as_str() > cursor.last.as_str())
            })
            .collect();

        let skip = if cursor.is_some() { 0 } else { offset };
        if skip >= matching.len() {
            return Ok(Page {
                items: Vec::new(),
                next: None,
            });
        }

        let available = matching.len() - skip;
        let take = limit.min(available);
        let items: Vec<(String, String)> = matching.into_iter().skip(skip).take(take).collect();
        let has_more = available > take;
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

        let visible = self.visible(tx, now);
        let mut count = 0usize;
        let mut sum = 0i64;

        for (key, value) in visible {
            if !Self::matches(&key, start, end, prefix) {
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

        if digits == b"0" {
            if bytes[0] == b'-' {
                return None;
            }
        } else {
            if digits[0] == b'0' || !digits.iter().all(u8::is_ascii_digit) {
                return None;
            }
            if bytes[0] != b'-' && !bytes[0].is_ascii_digit() {
                return None;
            }
        }

        if !digits.iter().all(u8::is_ascii_digit) {
            return None;
        }

        value.parse::<i64>().ok()
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

        let mut copy = BTreeMap::new();
        for (key, value) in self.visible(None, now) {
            if let Some(entry) = self
                .data
                .get(&key)
                .and_then(|versions| versions.last())
                .and_then(|version| version.entry.clone())
            {
                if Self::is_live(&entry, now) && entry.value == value {
                    copy.insert(key, entry);
                }
            }
        }
        self.snapshots.insert(name.to_string(), copy);
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
            .ok_or_else(|| Error::SnapshotMissing(name.to_string()))?;

        if !self.transactions.is_empty() {
            return Err(Error::ActiveTransactions);
        }

        let saved_live: BTreeMap<String, Entry> = saved
            .into_iter()
            .filter(|(_, entry)| Self::is_live(entry, now))
            .collect();

        let current_live = self.visible(None, now);
        let keys: BTreeSet<String> = current_live
            .keys()
            .cloned()
            .chain(saved_live.keys().cloned())
            .collect();

        if keys.is_empty() {
            return Ok(self.revision);
        }

        self.revision += 1;
        let revision = self.revision;
        for key in keys {
            let entry = saved_live.get(&key).cloned();
            self.append_version(key, entry, revision);
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
            return Err(Error::SnapshotMissing(name.to_string()));
        }
        Ok(())
    }

    pub fn gc(&mut self, now: u64) -> Result<usize, Error> {
        self.admit(now)?;
        let mut removed = 0usize;

        for versions in self.data.values_mut() {
            let mut keep = BTreeSet::new();
            if let Some(last) = versions.last() {
                keep.insert(last.revision);
            }
            for transaction in self.transactions.values() {
                if let Some(version) = versions
                    .iter()
                    .rev()
                    .find(|version| version.revision <= transaction.begin_revision)
                {
                    keep.insert(version.revision);
                }
            }
            let before = versions.len();
            versions.retain(|version| keep.contains(&version.revision));
            removed += before - versions.len();
        }
        Ok(removed)
    }

    pub fn stats(&mut self, now: u64) -> Result<Stats, Error> {
        self.admit(now)?;
        let keys = self.data.len();
        let versions = self.data.values().map(Vec::len).sum();
        let tombstones = self
            .data
            .values()
            .flat_map(|versions| versions.iter())
            .filter(|version| version.entry.is_none())
            .count();
        let live_keys = self.visible(None, now).len();
        let active_transactions = self.transactions.len();
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
            active_transactions,
            read_only_transactions,
            staged_writes,
            snapshots: self.snapshots.len(),
        })
    }
}