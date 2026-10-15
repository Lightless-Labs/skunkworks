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
struct SavedValue {
    value: String,
    deadline: Option<u64>,
}

pub struct Store {
    clock: u64,
    revision: u64,
    data: BTreeMap<String, Vec<Version>>,
    transactions: HashMap<u64, Transaction>,
    snapshots: HashMap<String, BTreeMap<String, SavedValue>>,
}

impl Store {
    pub fn new() -> Self {
        Self {
            clock: 0,
            revision: 0,
            data: BTreeMap::new(),
            transactions: HashMap::new(),
            snapshots: HashMap::new(),
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

    fn tx_check(&self, tx: Option<u64>) -> Result<(), Error> {
        if let Some(id) = tx {
            if !self.transactions.contains_key(&id) {
                return Err(Error::UnknownTransaction(id));
            }
        }
        Ok(())
    }

    fn selected_version(&self, key: &str, tx: Option<u64>) -> Option<&Version> {
        let versions = self.data.get(key)?;
        match tx {
            None => versions.last(),
            Some(id) => {
                let begin = self.transactions.get(&id)?.begin_revision;
                versions.iter().rev().find(|v| v.revision <= begin)
            }
        }
    }

    fn visible_value(&self, key: &str, tx: Option<u64>, now: u64) -> Option<String> {
        if let Some(id) = tx {
            if let Some(write) = self.transactions.get(&id)?.writes.get(key) {
                return match (&write.value, write.deadline) {
                    (Some(value), Some(deadline)) if deadline <= now => None,
                    (Some(value), _) => Some(value.clone()),
                    (None, _) => None,
                };
            }
        }
        let version = self.selected_version(key, tx)?;
        match (&version.value, version.deadline) {
            (Some(value), Some(deadline)) if deadline <= now => None,
            (Some(value), _) => Some(value.clone()),
            (None, _) => None,
        }
    }

    fn append(&mut self, key: String, write: Write) {
        self.revision += 1;
        self.append_at(key, write, self.revision);
    }

    fn append_at(&mut self, key: String, write: Write, revision: u64) {
        self.data.entry(key).or_default().push(Version {
            revision,
            value: write.value,
            deadline: write.deadline,
        });
    }

    fn validate_key(key: &str) -> Result<(), Error> {
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

    fn range_valid(start: &str, end: Option<&str>) -> Result<(), Error> {
        if end.is_some_and(|e| e <= start) {
            Err(Error::InvalidRange)
        } else {
            Ok(())
        }
    }

    fn matches(key: &str, start: &str, end: Option<&str>, prefix: Option<&str>) -> bool {
        key >= start
            && end.is_none_or(|e| key < e)
            && prefix.is_none_or(|p| key.starts_with(p))
    }

    fn visible_entries(
        &self,
        tx: Option<u64>,
        now: u64,
        start: &str,
        end: Option<&str>,
        prefix: Option<&str>,
    ) -> Vec<(String, String)> {
        let mut keys: BTreeSet<String> = self.data.keys().cloned().collect();
        if let Some(id) = tx {
            if let Some(t) = self.transactions.get(&id) {
                keys.extend(t.writes.keys().cloned());
            }
        }
        keys.into_iter()
            .filter(|key| Self::matches(key, start, end, prefix))
            .filter_map(|key| {
                self.visible_value(&key, tx, now)
                    .map(|value| (key, value))
            })
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
        let transaction = self
            .transactions
            .get(&id)
            .ok_or(Error::UnknownTransaction(id))?;

        if !transaction.writes.is_empty() {
            let begin_revision = transaction.begin_revision;
            for key in transaction.writes.keys() {
                if self
                    .data
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

            let writes = transaction.writes.clone();
            let next_revision = self.revision + 1;
            for (key, write) in writes {
                self.append_at(key, write, next_revision);
            }
            self.revision = next_revision;
        }

        self.transactions.remove(&id);
        Ok(self.revision)
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
        self.tx_check(tx)?;
        Self::validate_key(key)?;
        if let Some(id) = tx {
            if self.transactions.get(&id).unwrap().read_only {
                return Err(Error::ReadOnly(id));
            }
        }
        let deadline = match ttl {
            Some(0) => {
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
                    self.append(key.to_string(), write);
                }
                return Ok(());
            }
            Some(duration) => Some(now.checked_add(duration).ok_or(Error::DeadlineOverflow)?),
            None => None,
        };
        let write = Write {
            value: Some(value.to_string()),
            deadline,
        };
        if let Some(id) = tx {
            self.transactions
                .get_mut(&id)
                .unwrap()
                .writes
                .insert(key.to_string(), write);
        } else {
            self.append(key.to_string(), write);
        }
        Ok(())
    }

    pub fn delete(&mut self, now: u64, tx: Option<u64>, key: &str) -> Result<bool, Error> {
        self.admit(now)?;
        self.tx_check(tx)?;
        Self::validate_key(key)?;
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
            self.append(key.to_string(), write);
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
        self.tx_check(tx)?;
        Self::validate_key(key)?;
        Ok(self.visible_value(key, tx, now))
    }

    pub fn count(&mut self, now: u64, tx: Option<u64>, value: &str) -> Result<usize, Error> {
        self.admit(now)?;
        self.tx_check(tx)?;
        Ok(self
            .visible_entries(tx, now, "", None, None)
            .iter()
            .filter(|(_, v)| v == value)
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
        self.tx_check(tx)?;
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

        let mut entries = self.visible_entries(tx, now, start, end, prefix);
        if let Some(c) = cursor {
            entries.retain(|(key, _)| key > &c.last);
        } else {
            entries = entries.into_iter().skip(offset).collect();
        }
        let has_more = entries.len() > limit;
        let items: Vec<_> = entries.into_iter().take(limit).collect();
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
        self.tx_check(tx)?;
        Self::range_valid(start, end)?;

        let entries = self.visible_entries(tx, now, start, end, prefix);
        let mut sum = 0i64;
        for (key, value) in &entries {
            let number = parse_canonical_i64(value).ok_or_else(|| Error::InvalidNumber {
                key: key.clone(),
                value: value.clone(),
            })?;
            sum = sum
                .checked_add(number)
                .ok_or_else(|| Error::SumOverflow { key: key.clone() })?;
        }
        Ok(Aggregate {
            count: entries.len(),
            sum,
        })
    }

    pub fn snapshot(&mut self, now: u64, name: &str) -> Result<(), Error> {
        self.admit(now)?;
        Self::validate_name(name)?;
        if !self.snapshots.contains_key(name) && self.snapshots.len() >= 3 {
            return Err(Error::SnapshotLimit);
        }
        let saved = self
            .data
            .keys()
            .filter_map(|key| {
                let version = self.data.get(key)?.last()?;
                match (&version.value, version.deadline) {
                    (Some(value), Some(deadline)) if deadline <= now => None,
                    (Some(value), deadline) => Some((
                        key.clone(),
                        SavedValue {
                            value: value.clone(),
                            deadline,
                        },
                    )),
                    (None, _) => None,
                }
            })
            .collect();
        self.snapshots.insert(name.to_string(), saved);
        Ok(())
    }

    pub fn restore(&mut self, now: u64, name: &str) -> Result<u64, Error> {
        self.admit(now)?;
        if name.is_empty() {
            return Err(Error::NameEmpty);
        }
        let saved = self
            .snapshots
            .get(name)
            .cloned()
            .ok_or_else(|| Error::SnapshotMissing(name.to_string()))?;
        if !self.transactions.is_empty() {
            return Err(Error::ActiveTransactions);
        }

        let copied: BTreeMap<String, SavedValue> = saved
            .into_iter()
            .filter(|(_, entry)| entry.deadline.is_none_or(|deadline| deadline > now))
            .collect();
        let current: BTreeSet<String> = self
            .data
            .keys()
            .filter(|key| self.visible_value(key, None, now).is_some())
            .cloned()
            .collect();
        let union: BTreeSet<String> = current.union(&copied.keys().cloned().collect()).cloned().collect();
        if union.is_empty() {
            return Ok(self.revision);
        }

        let next_revision = self.revision + 1;
        for key in union {
            let write = if let Some(value) = copied.get(&key) {
                Write {
                    value: Some(value.value.clone()),
                    deadline: value.deadline,
                }
            } else {
                Write {
                    value: None,
                    deadline: None,
                }
            };
            self.append_at(key, write, next_revision);
        }
        self.revision = next_revision;
        Ok(self.revision)
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
        let begin_revisions: Vec<u64> = self
            .transactions
            .values()
            .map(|transaction| transaction.begin_revision)
            .collect();
        let mut removed = 0;
        for versions in self.data.values_mut() {
            let mut keep = BTreeSet::new();
            if !versions.is_empty() {
                keep.insert(versions.len() - 1);
            }
            for begin in &begin_revisions {
                if let Some(index) = versions.iter().rposition(|v| v.revision <= *begin) {
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
            .flat_map(|vs| vs.iter())
            .filter(|version| version.value.is_none())
            .count();
        let live_keys = self
            .data
            .keys()
            .filter(|key| self.visible_value(key, None, now).is_some())
            .count();
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
            keys: self.data.len(),
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

fn parse_canonical_i64(value: &str) -> Option<i64> {
    if value == "0" {
        return Some(0);
    }
    let digits = value.strip_prefix('-').unwrap_or(value);
    if digits.is_empty()
        || !digits.bytes().all(|byte| byte.is_ascii_digit())
        || digits.starts_with('0')
    {
        return None;
    }
    if value.starts_with('-') {
        if digits == "0" {
            return None;
        }
    }
    value.parse::<i64>().ok()
}

fn main() {}