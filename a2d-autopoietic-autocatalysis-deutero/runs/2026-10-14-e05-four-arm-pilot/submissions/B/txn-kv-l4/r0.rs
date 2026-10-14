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
    writes: std::collections::BTreeMap<String, Write>,
}

#[derive(Clone)]
struct Snapshot {
    entries: std::collections::BTreeMap<String, (String, Option<u64>)>,
}

pub struct Store {
    clock: u64,
    revision: u64,
    versions: std::collections::BTreeMap<String, Vec<Version>>,
    transactions: std::collections::BTreeMap<u64, Transaction>,
    snapshots: std::collections::BTreeMap<String, Snapshot>,
}

impl Store {
    pub fn new() -> Self {
        Self {
            clock: 0,
            revision: 0,
            versions: std::collections::BTreeMap::new(),
            transactions: std::collections::BTreeMap::new(),
            snapshots: std::collections::BTreeMap::new(),
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

    fn selected_version<'a>(
        &'a self,
        key: &str,
        revision: Option<u64>,
    ) -> Option<&'a Version> {
        let versions = self.versions.get(key)?;
        match revision {
            None => versions.last(),
            Some(r) => versions.iter().rev().find(|v| v.revision <= r),
        }
    }

    fn visible_value(&self, key: &str, tx: Option<u64>, now: u64) -> Option<String> {
        let transaction = tx.and_then(|id| self.transactions.get(&id));
        if let Some(t) = transaction {
            if let Some(w) = t.writes.get(key) {
                return Self::live_value(&w.value, w.deadline, now);
            }
        }
        let revision = transaction.map(|t| t.begin_revision);
        let version = self.selected_version(key, revision)?;
        Self::live_value(&version.value, version.deadline, now)
    }

    fn live_value(value: &Option<String>, deadline: Option<u64>, now: u64) -> Option<String> {
        if deadline.map_or(false, |d| d <= now) {
            return None;
        }
        value.clone()
    }

    fn visible_map(
        &self,
        tx: Option<u64>,
        now: u64,
    ) -> std::collections::BTreeMap<String, String> {
        let mut keys: std::collections::BTreeSet<String> =
            self.versions.keys().cloned().collect();
        if let Some(id) = tx {
            if let Some(t) = self.transactions.get(&id) {
                keys.extend(t.writes.keys().cloned());
            }
        }
        let mut result = std::collections::BTreeMap::new();
        for key in keys {
            if let Some(value) = self.visible_value(&key, tx, now) {
                result.insert(key, value);
            }
        }
        result
    }

    fn append_version(&mut self, key: String, write: Write, revision: u64) {
        self.versions.entry(key).or_default().push(Version {
            revision,
            value: write.value,
            deadline: write.deadline,
        });
    }

    fn valid_key(key: &str) -> Result<(), Error> {
        if key.is_empty() {
            Err(Error::KeyEmpty)
        } else {
            Ok(())
        }
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

    fn range_valid(start: &str, end: Option<&str>) -> Result<(), Error> {
        if end.map_or(false, |e| e <= start) {
            Err(Error::InvalidRange)
        } else {
            Ok(())
        }
    }

    fn matches(key: &str, start: &str, end: Option<&str>, prefix: Option<&str>) -> bool {
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
                writes: std::collections::BTreeMap::new(),
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
        if transaction.writes.is_empty() {
            self.transactions.remove(&id);
            return Ok(self.revision);
        }

        for key in transaction.writes.keys() {
            if self
                .versions
                .get(key)
                .and_then(|vs| vs.last())
                .map_or(false, |v| v.revision > transaction.begin_revision)
            {
                return Err(Error::Conflict {
                    tx: id,
                    key: key.clone(),
                });
            }
        }

        let writes = transaction.writes.clone();
        self.revision += 1;
        let revision = self.revision;
        for (key, write) in writes {
            self.append_version(key, write, revision);
        }
        self.transactions.remove(&id);
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
        self.check_tx(tx)?;
        Self::valid_key(key)?;
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
        Self::valid_key(key)?;
        if let Some(id) = tx {
            if self.transactions.get(&id).unwrap().read_only {
                return Err(Error::ReadOnly(id));
            }
        }

        let existed = self.visible_value(key, tx, now).is_some();
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
        Self::valid_key(key)?;
        Ok(self.visible_value(key, tx, now))
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
        self.check_tx(tx)?;
        Self::range_valid(start, end)?;

        if let Some(c) = cursor {
            if c.start != start
                || c.end.as_deref() != end
                || c.prefix.as_deref() != prefix
                || c.offset != offset
                || c.last.is_empty()
                || !Self::matches(&c.last, start, end, prefix)
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

        let map = self.visible_map(tx, now);
        let mut entries: Vec<(String, String)> = map
            .into_iter()
            .filter(|(k, _)| Self::matches(k, start, end, prefix))
            .filter(|(k, _)| cursor.map_or(true, |c| k.as_str() > c.last.as_str()))
            .collect();

        if cursor.is_none() {
            if offset >= entries.len() {
                return Ok(Page {
                    items: Vec::new(),
                    next: None,
                });
            }
            entries.drain(..offset);
        }

        let has_more = entries.len() > limit;
        let items: Vec<(String, String)> = entries.into_iter().take(limit).collect();
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
        Self::range_valid(start, end)?;

        let mut count = 0usize;
        let mut sum = 0i64;
        for (key, value) in self.visible_map(tx, now) {
            if !Self::matches(&key, start, end, prefix) {
                continue;
            }
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
        if value.is_empty() {
            return None;
        }
        let digits = if let Some(rest) = value.strip_prefix('-') {
            if rest.is_empty() || rest == "0" {
                return None;
            }
            rest
        } else {
            value
        };
        if digits == "0" {
            return Some(0);
        }
        if digits.starts_with('0') || !digits.bytes().all(|b| b.is_ascii_digit()) {
            return None;
        }
        if value.starts_with('-') {
            if !digits.bytes().all(|b| b.is_ascii_digit()) {
                return None;
            }
            value.parse::<i64>().ok()
        } else {
            value.parse::<i64>().ok()
        }
    }

    pub fn snapshot(&mut self, now: u64, name: &str) -> Result<(), Error> {
        self.admit(now)?;
        Self::validate_name(name)?;
        if !self.snapshots.contains_key(name) && self.snapshots.len() >= 3 {
            return Err(Error::SnapshotLimit);
        }
        let mut entries = std::collections::BTreeMap::new();
        for (key, versions) in &self.versions {
            if let Some(version) = versions.last() {
                if let Some(value) = Self::live_value(&version.value, version.deadline, now) {
                    entries.insert(key.clone(), (value, version.deadline));
                }
            }
        }
        self.snapshots
            .insert(name.to_string(), Snapshot { entries });
        Ok(())
    }

    pub fn restore(&mut self, now: u64, name: &str) -> Result<u64, Error> {
        self.admit(now)?;
        if name.is_empty() {
            return Err(Error::NameEmpty);
        }
        let snapshot = self
            .snapshots
            .get(name)
            .cloned()
            .ok_or_else(|| Error::SnapshotMissing(name.to_string()))?;
        if !self.transactions.is_empty() {
            return Err(Error::ActiveTransactions);
        }

        let copied: std::collections::BTreeMap<String, (String, Option<u64>)> = snapshot
            .entries
            .into_iter()
            .filter(|(_, (_, deadline))| deadline.map_or(true, |d| d > now))
            .collect();

        let current = self.visible_map(None, now);
        let keys: std::collections::BTreeSet<String> =
            current.keys().cloned().chain(copied.keys().cloned()).collect();
        if keys.is_empty() {
            return Ok(self.revision);
        }

        self.revision += 1;
        let revision = self.revision;
        for key in keys {
            let write = copied.get(&key).map_or(
                Write {
                    value: None,
                    deadline: None,
                },
                |(value, deadline)| Write {
                    value: Some(value.clone()),
                    deadline: *deadline,
                },
            );
            self.append_version(key, write, revision);
        }
        Ok(revision)
    }

    pub fn forget(&mut self, now: u64, name: &str) -> Result<(), Error> {
        self.admit(now)?;
        if name.is_empty() {
            return Err(Error::NameEmpty);
        }
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
            let mut keep = std::collections::BTreeSet::new();
            keep.insert(versions.len() - 1);
            for transaction in self.transactions.values() {
                if let Some(index) = versions
                    .iter()
                    .rposition(|v| v.revision <= transaction.begin_revision)
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
        let keys = self.versions.len();
        let versions = self.versions.values().map(Vec::len).sum();
        let tombstones = self
            .versions
            .values()
            .flat_map(|vs| vs.iter())
            .filter(|v| v.value.is_none())
            .count();
        let live_keys = self.visible_map(None, now).len();
        let active_transactions = self.transactions.len();
        let read_only_transactions = self
            .transactions
            .values()
            .filter(|t| t.read_only)
            .count();
        let staged_writes = self
            .transactions
            .values()
            .map(|t| t.writes.len())
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