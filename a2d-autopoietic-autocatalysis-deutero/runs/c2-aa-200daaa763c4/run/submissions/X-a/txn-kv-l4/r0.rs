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

pub struct Store {
    clock: u64,
    revision: u64,
    data: BTreeMap<String, Vec<Version>>,
    transactions: BTreeMap<u64, Transaction>,
    snapshots: BTreeMap<String, BTreeMap<String, Entry>>,
}

#[derive(Clone)]
struct Entry {
    value: String,
    deadline: Option<u64>,
}

struct Version {
    revision: u64,
    entry: Option<Entry>,
}

struct Transaction {
    begin_revision: u64,
    read_only: bool,
    writes: BTreeMap<String, Option<Entry>>,
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

    fn check_tx(&self, tx: Option<u64>) -> Result<(), Error> {
        if let Some(id) = tx {
            if !self.transactions.contains_key(&id) {
                return Err(Error::UnknownTransaction(id));
            }
        }
        Ok(())
    }

    fn selected_entry(&self, key: &str, tx: Option<u64>) -> Option<&Entry> {
        if let Some(id) = tx {
            let transaction = self.transactions.get(&id)?;
            if let Some(entry) = transaction.writes.get(key) {
                return entry.as_ref();
            }
            self.data
                .get(key)?
                .iter()
                .rev()
                .find(|version| version.revision <= transaction.begin_revision)?
                .entry
                .as_ref()
        } else {
            self.data.get(key)?.last()?.entry.as_ref()
        }
    }

    fn live_entry(&self, key: &str, tx: Option<u64>, now: u64) -> Option<&Entry> {
        self.selected_entry(key, tx)
            .filter(|entry| entry.deadline.map_or(true, |deadline| deadline > now))
    }

    fn visible(&self, tx: Option<u64>, now: u64) -> BTreeMap<String, Entry> {
        let mut keys: BTreeSet<&str> = self.data.keys().map(String::as_str).collect();
        if let Some(id) = tx {
            if let Some(transaction) = self.transactions.get(&id) {
                keys.extend(transaction.writes.keys().map(String::as_str));
            }
        }

        keys.into_iter()
            .filter_map(|key| {
                self.live_entry(key, tx, now)
                    .map(|entry| (key.to_owned(), entry.clone()))
            })
            .collect()
    }

    fn write_committed(&mut self, key: String, entry: Option<Entry>) {
        self.revision += 1;
        self.data.entry(key).or_default().push(Version {
            revision: self.revision,
            entry,
        });
    }

    fn valid_name(name: &str) -> Result<(), Error> {
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

    fn range_valid(start: &str, end: Option<&str>) -> Result<(), Error> {
        if end.is_some_and(|end| end <= start) {
            return Err(Error::InvalidRange);
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

        let writes = self.transactions.get(&id).unwrap().writes.clone();
        if writes.is_empty() {
            self.transactions.remove(&id);
            return Ok(self.revision);
        }

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
        self.check_tx(tx)?;
        if key.is_empty() {
            return Err(Error::KeyEmpty);
        }
        if let Some(id) = tx {
            if self.transactions[&id].read_only {
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

        if let Some(id) = tx {
            self.transactions
                .get_mut(&id)
                .unwrap()
                .writes
                .insert(key.to_owned(), entry);
        } else {
            self.write_committed(key.to_owned(), entry);
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
            if self.transactions[&id].read_only {
                return Err(Error::ReadOnly(id));
            }
        }

        let existed = self.live_entry(key, tx, now).is_some();
        if let Some(id) = tx {
            self.transactions
                .get_mut(&id)
                .unwrap()
                .writes
                .insert(key.to_owned(), None);
        } else {
            self.write_committed(key.to_owned(), None);
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
            .live_entry(key, tx, now)
            .map(|entry| entry.value.clone()))
    }

    pub fn count(&mut self, now: u64, tx: Option<u64>, value: &str) -> Result<usize, Error> {
        self.admit(now)?;
        self.check_tx(tx)?;
        Ok(self
            .visible(tx, now)
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
        Self::range_valid(start, end)?;

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

        let mut matches: Vec<(String, String)> = self
            .visible(tx, now)
            .into_iter()
            .filter(|(key, _)| key.as_str() >= start)
            .filter(|(key, _)| end.map_or(true, |end| key.as_str() < end))
            .filter(|(key, _)| prefix.map_or(true, |prefix| key.starts_with(prefix)))
            .filter(|(key, _)| cursor.map_or(true, |cursor| key.as_str() > cursor.last.as_str()))
            .map(|(key, entry)| (key, entry.value))
            .collect();

        if cursor.is_none() {
            let discarded = offset.min(matches.len());
            matches.drain(..discarded);
        }

        let take = limit.min(matches.len());
        let has_more = take < matches.len();
        let items: Vec<_> = matches.into_iter().take(take).collect();
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
        self.check_tx(tx)?;
        Self::range_valid(start, end)?;

        let mut count = 0;
        let mut sum = 0i64;
        for (key, entry) in self.visible(tx, now) {
            if key.as_str() < start
                || end.is_some_and(|end| key.as_str() >= end)
                || prefix.is_some_and(|prefix| !key.starts_with(prefix))
            {
                continue;
            }
            let value = parse_canonical_i64(&entry.value).ok_or_else(|| Error::InvalidNumber {
                key: key.clone(),
                value: entry.value.clone(),
            })?;
            sum = sum
                .checked_add(value)
                .ok_or_else(|| Error::SumOverflow { key: key.clone() })?;
            count += 1;
        }
        Ok(Aggregate { count, sum })
    }

    pub fn snapshot(&mut self, now: u64, name: &str) -> Result<(), Error> {
        self.admit(now)?;
        Self::valid_name(name)?;
        if self.snapshots.len() >= 3 && !self.snapshots.contains_key(name) {
            return Err(Error::SnapshotLimit);
        }
        self.snapshots.insert(name.to_owned(), self.visible(None, now));
        Ok(())
    }

    pub fn restore(&mut self, now: u64, name: &str) -> Result<u64, Error> {
        self.admit(now)?;
        Self::valid_name(name)?;
        let saved = self
            .snapshots
            .get(name)
            .ok_or_else(|| Error::SnapshotMissing(name.to_owned()))?;
        if !self.transactions.is_empty() {
            return Err(Error::ActiveTransactions);
        }

        let saved_live: BTreeMap<String, Entry> = saved
            .iter()
            .filter(|(_, entry)| entry.deadline.map_or(true, |deadline| deadline > now))
            .map(|(key, entry)| (key.clone(), entry.clone()))
            .collect();
        let current = self.visible(None, now);
        let mut keys: BTreeSet<String> = current.keys().cloned().collect();
        keys.extend(saved_live.keys().cloned());
        if keys.is_empty() {
            return Ok(self.revision);
        }

        self.revision += 1;
        let revision = self.revision;
        for key in keys {
            self.data.entry(key.clone()).or_default().push(Version {
                revision,
                entry: saved_live.get(&key).cloned(),
            });
        }
        Ok(revision)
    }

    pub fn forget(&mut self, now: u64, name: &str) -> Result<(), Error> {
        self.admit(now)?;
        Self::valid_name(name)?;
        if self.snapshots.remove(name).is_none() {
            return Err(Error::SnapshotMissing(name.to_owned()));
        }
        Ok(())
    }

    pub fn gc(&mut self, now: u64) -> Result<usize, Error> {
        self.admit(now)?;
        let mut removed = 0;
        for versions in self.data.values_mut() {
            let mut retained = BTreeSet::new();
            if !versions.is_empty() {
                retained.insert(versions.len() - 1);
            }
            for transaction in self.transactions.values() {
                if let Some(index) = versions
                    .iter()
                    .rposition(|version| version.revision <= transaction.begin_revision)
                {
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
        let versions = self.data.values().map(Vec::len).sum();
        let tombstones = self
            .data
            .values()
            .flat_map(|versions| versions.iter())
            .filter(|version| version.entry.is_none())
            .count();

        Ok(Stats {
            clock: self.clock,
            revision: self.revision,
            keys: self.data.values().filter(|versions| !versions.is_empty()).count(),
            live_keys: self.visible(None, now).len(),
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
    if value.is_empty() {
        return None;
    }

    let digits = if let Some(rest) = value.strip_prefix('-') {
        if rest.is_empty() || rest == "0" || !rest.bytes().all(|byte| byte.is_ascii_digit()) {
            return None;
        }
        rest
    } else {
        if !value.bytes().all(|byte| byte.is_ascii_digit())
            || (value.len() > 1 && value.starts_with('0'))
        {
            return None;
        }
        value
    };

    if digits.is_empty() || (digits.len() > 1 && digits.starts_with('0')) {
        return None;
    }
    value.parse::<i64>().ok()
}