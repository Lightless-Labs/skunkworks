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

#[derive(Debug, Clone, PartialEq, Eq)]
struct Entry {
    value: Option<String>,
    deadline: Option<u64>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct Version {
    revision: u64,
    entry: Entry,
}

#[derive(Debug)]
struct Transaction {
    begin_revision: u64,
    read_only: bool,
    writes: BTreeMap<String, Entry>,
}

#[derive(Debug)]
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

    fn tx_exists(&self, id: u64) -> Result<(), Error> {
        if self.transactions.contains_key(&id) {
            Ok(())
        } else {
            Err(Error::UnknownTransaction(id))
        }
    }

    fn visible_entry(&self, key: &str, tx: Option<u64>) -> Option<Entry> {
        if let Some(id) = tx {
            let transaction = self.transactions.get(&id)?;
            if let Some(entry) = transaction.writes.get(key) {
                return Some(entry.clone());
            }
            self.data
                .get(key)?
                .iter()
                .rev()
                .find(|version| version.revision <= transaction.begin_revision)
                .map(|version| version.entry.clone())
        } else {
            self.data
                .get(key)?
                .last()
                .map(|version| version.entry.clone())
        }
    }

    fn current_entry(&self, key: &str) -> Option<Entry> {
        self.data
            .get(key)
            .and_then(|versions| versions.last())
            .map(|version| version.entry.clone())
    }

    fn live(entry: &Entry, now: u64) -> Option<String> {
        match (&entry.value, entry.deadline) {
            (Some(value), deadline) if deadline.map_or(true, |d| d > now) => {
                Some(value.clone())
            }
            _ => None,
        }
    }

    fn visible_live_map(&self, tx: Option<u64>, now: u64) -> BTreeMap<String, String> {
        let mut keys: BTreeSet<String> = self.data.keys().cloned().collect();
        if let Some(id) = tx {
            if let Some(transaction) = self.transactions.get(&id) {
                keys.extend(transaction.writes.keys().cloned());
            }
        }

        let mut result = BTreeMap::new();
        for key in keys {
            if let Some(entry) = self.visible_entry(&key, tx) {
                if let Some(value) = Self::live(&entry, now) {
                    result.insert(key, value);
                }
            }
        }
        result
    }

    fn append_version(&mut self, key: String, entry: Entry, revision: u64) {
        self.data
            .entry(key)
            .or_default()
            .push(Version { revision, entry });
    }

    fn key_valid(key: &str) -> Result<(), Error> {
        if key.is_empty() {
            Err(Error::KeyEmpty)
        } else {
            Ok(())
        }
    }

    fn name_valid(name: &str) -> Result<(), Error> {
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
        if end.is_some_and(|end| end <= start) {
            Err(Error::InvalidRange)
        } else {
            Ok(())
        }
    }

    fn key_matches(key: &str, start: &str, end: Option<&str>, prefix: Option<&str>) -> bool {
        key >= start
            && end.map_or(true, |end| key < end)
            && prefix.map_or(true, |prefix| key.starts_with(prefix))
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
        self.tx_exists(id)?;

        let transaction = self.transactions.get(&id).expect("validated transaction");
        if !transaction.writes.is_empty() {
            let begin_revision = transaction.begin_revision;
            for key in transaction.writes.keys() {
                let newest_revision = self
                    .data
                    .get(key)
                    .and_then(|versions| versions.last())
                    .map(|version| version.revision);
                if newest_revision.is_some_and(|revision| revision > begin_revision) {
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
        }

        self.transactions.remove(&id);
        Ok(self.revision)
    }

    pub fn rollback(&mut self, now: u64, id: u64) -> Result<(), Error> {
        self.admit(now)?;
        self.tx_exists(id)?;
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
            self.tx_exists(id)?;
        }
        Self::key_valid(key)?;
        if let Some(id) = tx {
            if self.transactions.get(&id).expect("validated transaction").read_only {
                return Err(Error::ReadOnly(id));
            }
        }

        let entry = match ttl {
            Some(0) => Entry {
                value: None,
                deadline: None,
            },
            Some(duration) => Entry {
                value: Some(value.to_string()),
                deadline: Some(now.checked_add(duration).ok_or(Error::DeadlineOverflow)?),
            },
            None => Entry {
                value: Some(value.to_string()),
                deadline: None,
            },
        };

        if let Some(id) = tx {
            self.transactions
                .get_mut(&id)
                .expect("validated transaction")
                .writes
                .insert(key.to_string(), entry);
        } else {
            self.revision += 1;
            let revision = self.revision;
            self.append_version(key.to_string(), entry, revision);
        }
        Ok(())
    }

    pub fn delete(&mut self, now: u64, tx: Option<u64>, key: &str) -> Result<bool, Error> {
        self.admit(now)?;
        if let Some(id) = tx {
            self.tx_exists(id)?;
        }
        Self::key_valid(key)?;
        if let Some(id) = tx {
            if self.transactions.get(&id).expect("validated transaction").read_only {
                return Err(Error::ReadOnly(id));
            }
        }

        let was_live = self
            .visible_entry(key, tx)
            .and_then(|entry| Self::live(&entry, now))
            .is_some();

        let entry = Entry {
            value: None,
            deadline: None,
        };
        if let Some(id) = tx {
            self.transactions
                .get_mut(&id)
                .expect("validated transaction")
                .writes
                .insert(key.to_string(), entry);
        } else {
            self.revision += 1;
            let revision = self.revision;
            self.append_version(key.to_string(), entry, revision);
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
            self.tx_exists(id)?;
        }
        Self::key_valid(key)?;
        Ok(self
            .visible_entry(key, tx)
            .and_then(|entry| Self::live(&entry, now)))
    }

    pub fn count(
        &mut self,
        now: u64,
        tx: Option<u64>,
        value: &str,
    ) -> Result<usize, Error> {
        self.admit(now)?;
        if let Some(id) = tx {
            self.tx_exists(id)?;
        }
        Ok(self
            .visible_live_map(tx, now)
            .values()
            .filter(|stored| stored.as_str() == value)
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
            self.tx_exists(id)?;
        }
        Self::validate_range(start, end)?;

        if let Some(cursor) = cursor {
            if cursor.start != start
                || cursor.end.as_deref() != end
                || cursor.prefix.as_deref() != prefix
                || cursor.offset != offset
                || cursor.last.is_empty()
                || !Self::key_matches(&cursor.last, start, end, prefix)
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

        let visible = self.visible_live_map(tx, now);
        let mut matching: Vec<(String, String)> = visible
            .into_iter()
            .filter(|(key, _)| Self::key_matches(key, start, end, prefix))
            .filter(|(key, _)| cursor.map_or(true, |c| key > &c.last))
            .collect();

        if cursor.is_none() {
            if offset >= matching.len() {
                return Ok(Page {
                    items: Vec::new(),
                    next: None,
                });
            }
            matching.drain(..offset);
        }

        let has_more = matching.len() > limit;
        let items: Vec<(String, String)> = matching.into_iter().take(limit).collect();
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
        if let Some(id) = tx {
            self.tx_exists(id)?;
        }
        Self::validate_range(start, end)?;

        let visible = self.visible_live_map(tx, now);
        let mut count = 0usize;
        let mut sum = 0i64;

        for (key, value) in visible {
            if !Self::key_matches(&key, start, end, prefix) {
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
        Self::name_valid(name)?;

        if !self.snapshots.contains_key(name) && self.snapshots.len() >= 3 {
            return Err(Error::SnapshotLimit);
        }

        let mut saved = BTreeMap::new();
        for (key, entry) in &self.data {
            if let Some(version) = entry.last() {
                if Self::live(&version.entry, now).is_some() {
                    saved.insert(key.clone(), version.entry.clone());
                }
            }
        }
        self.snapshots.insert(name.to_string(), saved);
        Ok(())
    }

    pub fn restore(&mut self, now: u64, name: &str) -> Result<u64, Error> {
        self.admit(now)?;
        Self::name_valid(name)?;
        let saved = self
            .snapshots
            .get(name)
            .cloned()
            .ok_or_else(|| Error::SnapshotMissing(name.to_string()))?;
        if !self.transactions.is_empty() {
            return Err(Error::ActiveTransactions);
        }

        let copied: BTreeMap<String, Entry> = saved
            .into_iter()
            .filter(|(_, entry)| Self::live(entry, now).is_some())
            .collect();

        let mut union = BTreeSet::new();
        for (key, versions) in &self.data {
            if versions
                .last()
                .is_some_and(|version| Self::live(&version.entry, now).is_some())
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
            let entry = copied.get(&key).cloned().unwrap_or(Entry {
                value: None,
                deadline: None,
            });
            self.append_version(key, entry, revision);
        }
        Ok(revision)
    }

    pub fn forget(&mut self, now: u64, name: &str) -> Result<(), Error> {
        self.admit(now)?;
        Self::name_valid(name)?;
        if self.snapshots.remove(name).is_none() {
            return Err(Error::SnapshotMissing(name.to_string()));
        }
        Ok(())
    }

    pub fn gc(&mut self, now: u64) -> Result<usize, Error> {
        self.admit(now)?;

        let begins: Vec<u64> = self
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

            for begin in &begins {
                if let Some(index) = versions.iter().rposition(|version| version.revision <= *begin) {
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
            .filter(|version| version.entry.value.is_none())
            .count();
        let live_keys = self
            .data
            .values()
            .filter(|versions| {
                versions
                    .last()
                    .is_some_and(|version| Self::live(&version.entry, now).is_some())
            })
            .count();

        Ok(Stats {
            clock: self.clock,
            revision: self.revision,
            keys: self.data.values().filter(|versions| !versions.is_empty()).count(),
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
    if digits[0] == b'0' && (digits.len() != 1 || bytes[0] == b'-') {
        return None;
    }

    value.parse::<i64>().ok()
}

fn main() {}