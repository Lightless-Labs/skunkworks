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
struct SavedEntry {
    value: String,
    deadline: Option<u64>,
}

pub struct Store {
    clock: u64,
    revision: u64,
    versions: BTreeMap<String, Vec<Version>>,
    transactions: BTreeMap<u64, Transaction>,
    snapshots: BTreeMap<String, BTreeMap<String, SavedEntry>>,
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

    fn check_key(key: &str) -> Result<(), Error> {
        if key.is_empty() {
            Err(Error::KeyEmpty)
        } else {
            Ok(())
        }
    }

    fn selected_version<'a>(
        &'a self,
        key: &str,
        tx: Option<u64>,
    ) -> Option<&'a Version> {
        let versions = self.versions.get(key)?;
        match tx {
            None => versions.last(),
            Some(id) => {
                let begin_revision = self.transactions.get(&id)?.begin_revision;
                versions
                    .iter()
                    .rev()
                    .find(|version| version.revision <= begin_revision)
            }
        }
    }

    fn visible_write<'a>(&'a self, id: u64, key: &str) -> Option<&'a Write> {
        self.transactions.get(&id)?.writes.get(key)
    }

    fn is_live(value: &Option<String>, deadline: Option<u64>, now: u64) -> bool {
        value.is_some() && deadline.is_none_or(|d| d > now)
    }

    fn get_visible(&self, tx: Option<u64>, key: &str, now: u64) -> Option<String> {
        if let Some(id) = tx {
            if let Some(write) = self.visible_write(id, key) {
                if Self::is_live(&write.value, write.deadline, now) {
                    return write.value.clone();
                }
                return None;
            }
        }
        let version = self.selected_version(key, tx)?;
        if Self::is_live(&version.value, version.deadline, now) {
            version.value.clone()
        } else {
            None
        }
    }

    fn append_version(&mut self, key: String, write: Write, revision: u64) {
        self.versions.entry(key).or_default().push(Version {
            revision,
            value: write.value,
            deadline: write.deadline,
        });
    }

    fn live_committed(&self, now: u64) -> BTreeMap<String, SavedEntry> {
        let mut result = BTreeMap::new();
        for (key, versions) in &self.versions {
            if let Some(version) = versions.last() {
                if Self::is_live(&version.value, version.deadline, now) {
                    result.insert(
                        key.clone(),
                        SavedEntry {
                            value: version.value.clone().unwrap(),
                            deadline: version.deadline,
                        },
                    );
                }
            }
        }
        result
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
        let tx = self
            .transactions
            .get(&id)
            .ok_or(Error::UnknownTransaction(id))?;

        if tx.writes.is_empty() {
            self.transactions.remove(&id);
            return Ok(self.revision);
        }

        let begin_revision = tx.begin_revision;
        for key in tx.writes.keys() {
            if self
                .versions
                .get(key)
                .and_then(|versions| versions.last())
                .is_some_and(|version| version.revision > begin_revision)
            {
                return Err(Error::Conflict {
                    tx: id,
                    key: key.clone(),
                });
            }
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
        Self::check_key(key)?;
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
                value: Some(value.to_string()),
                deadline: Some(now.checked_add(duration).ok_or(Error::DeadlineOverflow)?),
            },
            None => Write {
                value: Some(value.to_string()),
                deadline: None,
            },
        };

        if let Some(id) = tx {
            self.transactions
                .get_mut(&id)
                .unwrap()
                .writes
                .insert(key.to_string(), write);
        } else {
            self.revision += 1;
            let revision = self.revision;
            self.append_version(key.to_string(), write, revision);
        }
        Ok(())
    }

    pub fn delete(&mut self, now: u64, tx: Option<u64>, key: &str) -> Result<bool, Error> {
        self.admit(now)?;
        self.check_tx(tx)?;
        Self::check_key(key)?;
        if let Some(id) = tx {
            if self.transactions.get(&id).unwrap().read_only {
                return Err(Error::ReadOnly(id));
            }
        }

        let was_live = self.get_visible(tx, key, now).is_some();
        let write = Write {
            value: None,
            deadline: None,
        };
        if let Some(id) = tx {
            self.transactions
                .get_mut(&id)
                .unwrap()
                .writes
                .insert(key.to_string(), write);
        } else {
            self.revision += 1;
            let revision = self.revision;
            self.append_version(key.to_string(), write, revision);
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
        self.check_tx(tx)?;
        Self::check_key(key)?;
        Ok(self.get_visible(tx, key, now))
    }

    fn matching_entries(
        &self,
        tx: Option<u64>,
        now: u64,
        start: &str,
        end: Option<&str>,
        prefix: Option<&str>,
        after: Option<&str>,
    ) -> Vec<(String, String)> {
        let mut keys = BTreeSet::new();
        keys.extend(self.versions.keys().cloned());
        if let Some(id) = tx {
            keys.extend(self.transactions.get(&id).unwrap().writes.keys().cloned());
        }

        keys.into_iter()
            .filter(|key| {
                key.as_str() >= start
                    && end.is_none_or(|end| key.as_str() < end)
                    && prefix.is_none_or(|prefix| key.starts_with(prefix))
                    && after.is_none_or(|last| key.as_str() > last)
            })
            .filter_map(|key| {
                self.get_visible(tx, &key, now)
                    .map(|value| (key, value))
            })
            .collect()
    }

    pub fn count(
        &mut self,
        now: u64,
        tx: Option<u64>,
        value: &str,
    ) -> Result<usize, Error> {
        self.admit(now)?;
        self.check_tx(tx)?;
        let keys: BTreeSet<String> = self
            .versions
            .keys()
            .cloned()
            .chain(
                tx.into_iter()
                    .flat_map(|id| self.transactions.get(&id).unwrap().writes.keys().cloned()),
            )
            .collect();

        Ok(keys
            .iter()
            .filter(|key| self.get_visible(tx, key, now).as_deref() == Some(value))
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
        if let Some(end_value) = end {
            if end_value <= start {
                return Err(Error::InvalidRange);
            }
        }

        let after = if let Some(cursor) = cursor {
            if cursor.start != start
                || cursor.end.as_deref() != end
                || cursor.prefix.as_deref() != prefix
                || cursor.offset != offset
                || cursor.last.is_empty()
                || cursor.last.as_str() < start
                || end.is_some_and(|end_value| cursor.last.as_str() >= end_value)
                || prefix.is_some_and(|prefix_value| !cursor.last.starts_with(prefix_value))
            {
                return Err(Error::CursorMismatch);
            }
            Some(cursor.last.as_str())
        } else {
            None
        };

        if limit == 0 {
            return Ok(Page {
                items: Vec::new(),
                next: None,
            });
        }

        let entries = self.matching_entries(tx, now, start, end, prefix, after);
        let start_index = if cursor.is_some() {
            0
        } else {
            offset.min(entries.len())
        };
        let available = &entries[start_index..];
        let take = limit.min(available.len());
        let items = available[..take].to_vec();
        let next = if take > 0 && take < available.len() {
            let last = items.last().unwrap().0.clone();
            Some(Cursor {
                start: start.to_string(),
                end: end.map(str::to_string),
                prefix: prefix.map(str::to_string),
                offset,
                last,
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
        if let Some(end_value) = end {
            if end_value <= start {
                return Err(Error::InvalidRange);
            }
        }

        let entries = self.matching_entries(tx, now, start, end, prefix, None);
        let mut count = 0usize;
        let mut sum = 0i64;
        for (key, value) in entries {
            let parsed = Self::parse_number(&key, &value)?;
            sum = sum
                .checked_add(parsed)
                .ok_or_else(|| Error::SumOverflow { key: key.clone() })?;
            count += 1;
        }
        Ok(Aggregate { count, sum })
    }

    fn parse_number(key: &str, value: &str) -> Result<i64, Error> {
        let bytes = value.as_bytes();
        let valid = if bytes == b"0" {
            true
        } else if bytes.first() == Some(&b'-') {
            bytes.len() >= 2
                && bytes[1] >= b'1'
                && bytes[1] <= b'9'
                && bytes[2..].iter().all(u8::is_ascii_digit)
        } else {
            !bytes.is_empty()
                && bytes[0] >= b'1'
                && bytes[0] <= b'9'
                && bytes[1..].iter().all(u8::is_ascii_digit)
        };

        if !valid {
            return Err(Error::InvalidNumber {
                key: key.to_string(),
                value: value.to_string(),
            });
        }
        value.parse::<i64>().map_err(|_| Error::InvalidNumber {
            key: key.to_string(),
            value: value.to_string(),
        })
    }

    fn validate_name(name: &str) -> Result<(), Error> {
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

    pub fn snapshot(&mut self, now: u64, name: &str) -> Result<(), Error> {
        self.admit(now)?;
        Self::validate_name(name)?;
        if !self.snapshots.contains_key(name) && self.snapshots.len() >= 3 {
            return Err(Error::SnapshotLimit);
        }
        let data = self.live_committed(now);
        self.snapshots.insert(name.to_string(), data);
        Ok(())
    }

    pub fn restore(&mut self, now: u64, name: &str) -> Result<u64, Error> {
        self.admit(now)?;
        Self::validate_name(name)?;
        let saved = self
            .snapshots
            .get(name)
            .ok_or_else(|| Error::SnapshotMissing(name.to_string()))?;

        if !self.transactions.is_empty() {
            return Err(Error::ActiveTransactions);
        }

        let saved_live: BTreeMap<String, SavedEntry> = saved
            .iter()
            .filter(|(_, entry)| entry.deadline.is_none_or(|deadline| deadline > now))
            .map(|(key, entry)| (key.clone(), entry.clone()))
            .collect();
        let current_live = self.live_committed(now);
        let mut union: BTreeSet<String> = current_live.keys().cloned().collect();
        union.extend(saved_live.keys().cloned());

        if union.is_empty() {
            return Ok(self.revision);
        }

        self.revision += 1;
        let revision = self.revision;
        for key in union {
            let write = if let Some(entry) = saved_live.get(&key) {
                Write {
                    value: Some(entry.value.clone()),
                    deadline: entry.deadline,
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
        Self::validate_name(name)?;
        if self.snapshots.remove(name).is_none() {
            return Err(Error::SnapshotMissing(name.to_string()));
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

            for tx in self.transactions.values() {
                if let Some(index) = versions
                    .iter()
                    .rposition(|version| version.revision <= tx.begin_revision)
                {
                    keep.insert(index);
                }
            }

            let mut index = 0usize;
            versions.retain(|_| {
                let retain = keep.contains(&index);
                index += 1;
                retain
            });
            removed += index - versions.len();
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

        Ok(Stats {
            clock: self.clock,
            revision: self.revision,
            keys: self.versions.len(),
            live_keys: self.live_committed(now).len(),
            versions,
            tombstones,
            active_transactions: self.transactions.len(),
            read_only_transactions: self
                .transactions
                .values()
                .filter(|tx| tx.read_only)
                .count(),
            staged_writes: self
                .transactions
                .values()
                .map(|tx| tx.writes.len())
                .sum(),
            snapshots: self.snapshots.len(),
        })
    }
}

impl Default for Store {
    fn default() -> Self {
        Self::new()
    }
}