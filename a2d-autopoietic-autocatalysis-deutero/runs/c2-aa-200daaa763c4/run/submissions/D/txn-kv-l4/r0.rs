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

#[derive(Debug, Clone, PartialEq, Eq)]
struct Entry {
    value: String,
    deadline: Option<u64>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum Stored {
    Value(Entry),
    Tombstone,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct Version {
    revision: u64,
    stored: Stored,
}

#[derive(Debug)]
struct Transaction {
    begin_revision: u64,
    read_only: bool,
    writes: BTreeMap<String, Stored>,
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

    fn check_key(key: &str) -> Result<(), Error> {
        if key.is_empty() {
            Err(Error::KeyEmpty)
        } else {
            Ok(())
        }
    }

    fn newest_version(&self, key: &str, at_revision: Option<u64>) -> Option<&Version> {
        let versions = self.data.get(key)?;
        versions
            .iter()
            .rev()
            .find(|v| at_revision.map_or(true, |revision| v.revision <= revision))
    }

    fn visible_stored(&self, tx: Option<u64>, key: &str) -> Option<Stored> {
        if let Some(id) = tx {
            let transaction = self.transactions.get(&id)?;
            if let Some(stored) = transaction.writes.get(key) {
                return Some(stored.clone());
            }
            return self
                .newest_version(key, Some(transaction.begin_revision))
                .map(|v| v.stored.clone());
        }
        self.newest_version(key, None).map(|v| v.stored.clone())
    }

    fn live_entry(stored: Option<Stored>, now: u64) -> Option<Entry> {
        match stored {
            Some(Stored::Value(entry))
                if entry.deadline.map_or(true, |deadline| deadline > now) =>
            {
                Some(entry)
            }
            _ => None,
        }
    }

    fn visible_entries(&self, tx: Option<u64>, now: u64) -> BTreeMap<String, Entry> {
        let mut keys: BTreeSet<String> = self.data.keys().cloned().collect();
        if let Some(id) = tx {
            if let Some(transaction) = self.transactions.get(&id) {
                keys.extend(transaction.writes.keys().cloned());
            }
        }

        keys.into_iter()
            .filter_map(|key| {
                let entry = Self::live_entry(self.visible_stored(tx, &key), now)?;
                Some((key, entry))
            })
            .collect()
    }

    fn write_committed(&mut self, key: String, stored: Stored) -> u64 {
        self.revision += 1;
        self.data.entry(key).or_default().push(Version {
            revision: self.revision,
            stored,
        });
        self.revision
    }

    fn selected_entries(
        &self,
        tx: Option<u64>,
        now: u64,
        start: &str,
        end: Option<&str>,
        prefix: Option<&str>,
    ) -> BTreeMap<String, Entry> {
        self.visible_entries(tx, now)
            .into_iter()
            .filter(|(key, _)| {
                key.as_str() >= start
                    && end.map_or(true, |end| key.as_str() < end)
                    && prefix.map_or(true, |prefix| key.starts_with(prefix))
            })
            .collect()
    }

    fn validate_range(start: &str, end: Option<&str>) -> Result<(), Error> {
        if end.is_some_and(|end| end <= start) {
            Err(Error::InvalidRange)
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
                .newest_version(key, None)
                .is_some_and(|version| version.revision > transaction.begin_revision)
            {
                return Err(Error::Conflict {
                    tx: id,
                    key: key.clone(),
                });
            }
        }

        let transaction = self.transactions.remove(&id).expect("checked transaction");
        if transaction.writes.is_empty() {
            return Ok(self.revision);
        }

        self.revision += 1;
        let revision = self.revision;
        for (key, stored) in transaction.writes {
            self.data.entry(key).or_default().push(Version {
                revision,
                stored,
            });
        }
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
        Self::check_key(key)?;

        if let Some(id) = tx {
            if self.transactions.get(&id).unwrap().read_only {
                return Err(Error::ReadOnly(id));
            }
        }

        let stored = match ttl {
            Some(0) => Stored::Tombstone,
            Some(duration) => Stored::Value(Entry {
                value: value.to_owned(),
                deadline: Some(now.checked_add(duration).ok_or(Error::DeadlineOverflow)?),
            }),
            None => Stored::Value(Entry {
                value: value.to_owned(),
                deadline: None,
            }),
        };

        if let Some(id) = tx {
            self.transactions
                .get_mut(&id)
                .unwrap()
                .writes
                .insert(key.to_owned(), stored);
        } else {
            self.write_committed(key.to_owned(), stored);
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

        let existed = Self::live_entry(self.visible_stored(tx, key), now).is_some();
        if let Some(id) = tx {
            self.transactions
                .get_mut(&id)
                .unwrap()
                .writes
                .insert(key.to_owned(), Stored::Tombstone);
        } else {
            self.write_committed(key.to_owned(), Stored::Tombstone);
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
        Self::check_key(key)?;
        Ok(Self::live_entry(self.visible_stored(tx, key), now).map(|entry| entry.value))
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
            .visible_entries(tx, now)
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
                || cursor.last.as_str() < start
                || end.is_some_and(|end| cursor.last.as_str() >= end)
                || prefix.is_some_and(|prefix| !cursor.last.starts_with(prefix))
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

        let selected = self.selected_entries(tx, now, start, end, prefix);
        let keys: Vec<String> = selected
            .keys()
            .filter(|key| cursor.map_or(true, |c| key.as_str() > c.last.as_str()))
            .cloned()
            .collect();

        let begin = if cursor.is_some() {
            0
        } else {
            offset.min(keys.len())
        };
        let finish = begin.saturating_add(limit).min(keys.len());
        let items: Vec<(String, String)> = keys[begin..finish]
            .iter()
            .map(|key| (key.clone(), selected[key].value.clone()))
            .collect();

        let next = if finish < keys.len() && !items.is_empty() {
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
        self.check_tx(tx)?;
        Self::validate_range(start, end)?;

        let selected = self.selected_entries(tx, now, start, end, prefix);
        let mut count = 0usize;
        let mut sum = 0i64;

        for (key, entry) in selected {
            let parsed =
                parse_canonical_i64(&entry.value).ok_or_else(|| Error::InvalidNumber {
                    key: key.clone(),
                    value: entry.value.clone(),
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
        Self::validate_name(name)?;

        if !self.snapshots.contains_key(name) && self.snapshots.len() >= 3 {
            return Err(Error::SnapshotLimit);
        }

        let live = self.visible_entries(None, now);
        self.snapshots.insert(name.to_owned(), live);
        Ok(())
    }

    pub fn restore(&mut self, now: u64, name: &str) -> Result<u64, Error> {
        self.admit(now)?;
        Self::validate_name(name)?;
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
            .filter(|(_, entry)| entry.deadline.map_or(true, |deadline| deadline > now))
            .collect();
        let current = self.visible_entries(None, now);
        let union: BTreeSet<String> = current.keys().chain(saved_live.keys()).cloned().collect();

        if union.is_empty() {
            return Ok(self.revision);
        }

        self.revision += 1;
        let revision = self.revision;
        for key in union {
            let stored = match saved_live.get(&key) {
                Some(entry) => Stored::Value(entry.clone()),
                None => Stored::Tombstone,
            };
            self.data.entry(key).or_default().push(Version {
                revision,
                stored,
            });
        }
        Ok(revision)
    }

    pub fn forget(&mut self, now: u64, name: &str) -> Result<(), Error> {
        self.admit(now)?;
        Self::validate_name(name)?;
        if self.snapshots.remove(name).is_none() {
            return Err(Error::SnapshotMissing(name.to_owned()));
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

        let mut removed = 0usize;
        for versions in self.data.values_mut() {
            let mut keep = BTreeSet::new();
            if let Some(last) = versions.len().checked_sub(1) {
                keep.insert(last);
            }

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

        let versions = self.data.values().map(Vec::len).sum();
        let tombstones = self
            .data
            .values()
            .flatten()
            .filter(|version| matches!(version.stored, Stored::Tombstone))
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
            live_keys: self.visible_entries(None, now).len(),
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

    let bytes = value.as_bytes();
    if bytes.is_empty() {
        return None;
    }

    let digit_start = if bytes[0] == b'-' { 1 } else { 0 };
    if digit_start == bytes.len() || bytes[digit_start] == b'0' {
        return None;
    }
    if !bytes[digit_start..].iter().all(u8::is_ascii_digit) {
        return None;
    }

    value.parse::<i64>().ok()
}