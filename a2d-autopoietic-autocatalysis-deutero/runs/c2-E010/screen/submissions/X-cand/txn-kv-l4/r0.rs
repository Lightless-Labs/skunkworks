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

    fn validate_tx(&self, tx: Option<u64>) -> Result<(), Error> {
        if let Some(id) = tx {
            if !self.transactions.contains_key(&id) {
                return Err(Error::UnknownTransaction(id));
            }
        }
        Ok(())
    }

    fn validate_key(key: &str) -> Result<(), Error> {
        if key.is_empty() {
            Err(Error::KeyEmpty)
        } else {
            Ok(())
        }
    }

    fn live(entry: &Entry, now: u64) -> bool {
        match entry.deadline {
            Some(deadline) => deadline > now,
            None => true,
        }
    }

    fn committed_view(&self, key: &str, tx: Option<u64>) -> Option<&Version> {
        let versions = self.data.get(key)?;
        if let Some(id) = tx {
            let begin = self.transactions.get(&id)?.begin_revision;
            versions.iter().rev().find(|v| v.revision <= begin)
        } else {
            versions.last()
        }
    }

    fn get_entry(&self, key: &str, tx: Option<u64>, now: u64) -> Option<&Entry> {
        if let Some(id) = tx {
            if let Some(write) = self.transactions.get(&id)?.writes.get(key) {
                return write.as_ref().filter(|e| Self::live(e, now));
            }
        }
        self.committed_view(key, tx)?
            .entry
            .as_ref()
            .filter(|e| Self::live(e, now))
    }

    fn all_candidate_keys(&self, tx: Option<u64>) -> BTreeSet<String> {
        let mut result: BTreeSet<String> = self.data.keys().cloned().collect();
        if let Some(id) = tx {
            if let Some(t) = self.transactions.get(&id) {
                result.extend(t.writes.keys().cloned());
            }
        }
        result
    }

    fn selected_keys(
        &self,
        tx: Option<u64>,
        now: u64,
        start: &str,
        end: Option<&str>,
        prefix: Option<&str>,
        after: Option<&str>,
    ) -> Vec<String> {
        self.all_candidate_keys(tx)
            .into_iter()
            .filter(|key| {
                key.as_str() >= start
                    && end.map_or(true, |bound| key.as_str() < bound)
                    && prefix.map_or(true, |p| key.starts_with(p))
                    && after.map_or(true, |a| key.as_str() > a)
                    && self.get_entry(key, tx, now).is_some()
            })
            .collect()
    }

    fn validate_name(name: &str) -> Result<(), Error> {
        if name.is_empty() {
            return Err(Error::NameEmpty);
        }
        for (at, ch) in name.char_indices() {
            let ok = if at == 0 {
                ch.is_ascii_lowercase()
            } else {
                ch.is_ascii_lowercase() || ch.is_ascii_digit() || ch == '_'
            };
            if !ok {
                return Err(Error::NameInvalid { at });
            }
        }
        Ok(())
    }

    fn validate_range(start: &str, end: Option<&str>) -> Result<(), Error> {
        match end {
            Some(end) if end <= start => Err(Error::InvalidRange),
            _ => Ok(()),
        }
    }

    fn committed_live(&self, now: u64) -> BTreeMap<String, Entry> {
        let mut live = BTreeMap::new();
        for (key, versions) in &self.data {
            if let Some(entry) = versions.last().and_then(|v| v.entry.as_ref()) {
                if Self::live(entry, now) {
                    live.insert(key.clone(), entry.clone());
                }
            }
        }
        live
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
        for key in tx.writes.keys() {
            let newest = self.data.get(key).and_then(|versions| versions.last());
            if newest.is_some_and(|version| version.revision > tx.begin_revision) {
                return Err(Error::Conflict {
                    tx: id,
                    key: key.clone(),
                });
            }
        }

        if tx.writes.is_empty() {
            self.transactions.remove(&id);
            return Ok(self.revision);
        }

        let writes = tx.writes.clone();
        self.revision += 1;
        let revision = self.revision;
        for (key, entry) in writes {
            self.data
                .entry(key)
                .or_default()
                .push(Version { revision, entry });
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
        self.validate_tx(tx)?;
        Self::validate_key(key)?;
        if let Some(id) = tx {
            if self.transactions.get(&id).unwrap().read_only {
                return Err(Error::ReadOnly(id));
            }
        }

        let value_to_write = match ttl {
            Some(0) => None,
            Some(delta) => Some(Entry {
                value: value.to_string(),
                deadline: Some(now.checked_add(delta).ok_or(Error::DeadlineOverflow)?),
            }),
            None => Some(Entry {
                value: value.to_string(),
                deadline: None,
            }),
        };

        if let Some(id) = tx {
            self.transactions
                .get_mut(&id)
                .unwrap()
                .writes
                .insert(key.to_string(), value_to_write);
        } else {
            self.revision += 1;
            self.data.entry(key.to_string()).or_default().push(Version {
                revision: self.revision,
                entry: value_to_write,
            });
        }
        Ok(())
    }

    pub fn delete(&mut self, now: u64, tx: Option<u64>, key: &str) -> Result<bool, Error> {
        self.admit(now)?;
        self.validate_tx(tx)?;
        Self::validate_key(key)?;
        if let Some(id) = tx {
            if self.transactions.get(&id).unwrap().read_only {
                return Err(Error::ReadOnly(id));
            }
        }

        let existed = self.get_entry(key, tx, now).is_some();
        if let Some(id) = tx {
            self.transactions
                .get_mut(&id)
                .unwrap()
                .writes
                .insert(key.to_string(), None);
        } else {
            self.revision += 1;
            self.data.entry(key.to_string()).or_default().push(Version {
                revision: self.revision,
                entry: None,
            });
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
        Self::validate_key(key)?;
        Ok(self
            .get_entry(key, tx, now)
            .map(|entry| entry.value.to_string()))
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
            .selected_keys(tx, now, "", None, None, None)
            .iter()
            .filter(|key| self.get_entry(key, tx, now).is_some_and(|e| e.value == value))
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
        Self::validate_range(start, end)?;

        let after = match cursor {
            Some(cursor) => {
                if cursor.start != start
                    || cursor.end.as_deref() != end
                    || cursor.prefix.as_deref() != prefix
                    || cursor.offset != offset
                    || cursor.last.is_empty()
                    || cursor.last.as_str() < start
                    || end.is_some_and(|bound| cursor.last.as_str() >= bound)
                    || prefix.is_some_and(|p| !cursor.last.starts_with(p))
                {
                    return Err(Error::CursorMismatch);
                }
                Some(cursor.last.as_str())
            }
            None => None,
        };

        if limit == 0 {
            return Ok(Page {
                items: Vec::new(),
                next: None,
            });
        }

        let keys = self.selected_keys(tx, now, start, end, prefix, after);
        let skip = if cursor.is_some() { 0 } else { offset };
        let mut items = Vec::new();
        for key in keys.iter().skip(skip).take(limit) {
            if let Some(entry) = self.get_entry(key, tx, now) {
                items.push((key.clone(), entry.value.clone()));
            }
        }

        let next = items.last().and_then(|(last, _)| {
            if keys.iter().any(|key| key > last) {
                Some(Cursor {
                    start: start.to_owned(),
                    end: end.map(str::to_owned),
                    prefix: prefix.map(str::to_owned),
                    offset,
                    last: last.clone(),
                })
            } else {
                None
            }
        });

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
        let keys = self.selected_keys(tx, now, start, end, prefix, None);
        let mut count = 0usize;
        let mut sum = 0i64;
        for key in keys {
            let value = self.get_entry(&key, tx, now).unwrap().value.clone();
            let number = parse_number(&value).ok_or_else(|| Error::InvalidNumber {
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
        self.admit(now)?;
        Self::validate_name(name)?;
        if !self.snapshots.contains_key(name) && self.snapshots.len() >= 3 {
            return Err(Error::SnapshotLimit);
        }
        let saved = self.committed_live(now);
        self.snapshots.insert(name.to_string(), saved);
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

        let saved_live: BTreeMap<String, Entry> = saved
            .iter()
            .filter(|(_, entry)| Self::live(entry, now))
            .map(|(key, entry)| (key.clone(), entry.clone()))
            .collect();
        let current_live = self.committed_live(now);
        let union: BTreeSet<String> = current_live
            .keys()
            .chain(saved_live.keys())
            .cloned()
            .collect();

        if union.is_empty() {
            return Ok(self.revision);
        }
        self.revision += 1;
        let revision = self.revision;
        for key in union {
            self.data
                .entry(key.clone())
                .or_default()
                .push(Version {
                    revision,
                    entry: saved_live.get(&key).cloned(),
                });
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
        let pins: Vec<u64> = self
            .transactions
            .values()
            .map(|transaction| transaction.begin_revision)
            .collect();
        let mut removed = 0usize;
        for versions in self.data.values_mut() {
            let mut retained = BTreeSet::new();
            if let Some(last) = versions.len().checked_sub(1) {
                retained.insert(last);
            }
            for revision in &pins {
                if let Some(index) = versions.iter().rposition(|v| v.revision <= *revision) {
                    retained.insert(index);
                }
            }
            let mut index = 0;
            versions.retain(|_| {
                let keep = retained.contains(&index);
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
        let mut live_keys = 0;
        let mut versions = 0;
        let mut tombstones = 0;
        for revisions in self.data.values() {
            versions += revisions.len();
            tombstones += revisions.iter().filter(|v| v.entry.is_none()).count();
            if revisions
                .last()
                .and_then(|v| v.entry.as_ref())
                .is_some_and(|entry| Self::live(entry, now))
            {
                live_keys += 1;
            }
        }
        Ok(Stats {
            clock: self.clock,
            revision: self.revision,
            keys: self.data.len(),
            live_keys,
            versions,
            tombstones,
            active_transactions: self.transactions.len(),
            read_only_transactions: self.transactions.values().filter(|t| t.read_only).count(),
            staged_writes: self.transactions.values().map(|t| t.writes.len()).sum(),
            snapshots: self.snapshots.len(),
        })
    }
}

fn parse_number(s: &str) -> Option<i64> {
    if s.is_empty() {
        return None;
    }

    let bytes = s.as_bytes();
    let digit_start = usize::from(bytes[0] == b'-');
    if digit_start == bytes.len() {
        return None;
    }
    let digits = &bytes[digit_start..];
    if !digits.iter().all(u8::is_ascii_digit) {
        return None;
    }
    if digits.len() > 1 && digits[0] == b'0' {
        return None;
    }
    if digit_start == 1 && digits == b"0" {
        return None;
    }
    if bytes[0] == b'+' {
        return None;
    }
    s.parse::<i64>().ok()
}
