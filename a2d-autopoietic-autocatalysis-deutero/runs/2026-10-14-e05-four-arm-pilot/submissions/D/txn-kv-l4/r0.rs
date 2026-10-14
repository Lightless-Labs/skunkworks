use std::collections::{BTreeMap, HashMap};

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
struct Value {
    value: String,
    deadline: Option<u64>,
}

#[derive(Clone)]
struct Version {
    revision: u64,
    value: Option<Value>,
}

struct Transaction {
    begin_revision: u64,
    read_only: bool,
    writes: BTreeMap<String, Option<Value>>,
}

pub struct Store {
    clock: u64,
    revision: u64,
    data: BTreeMap<String, Vec<Version>>,
    transactions: HashMap<u64, Transaction>,
    snapshots: BTreeMap<String, BTreeMap<String, Value>>,
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

    fn selected_version(&self, key: &str, tx: Option<u64>) -> Option<&Version> {
        let versions = self.data.get(key)?;
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

    fn live_value(value: Option<&Value>, now: u64) -> Option<String> {
        let value = value?;
        if value.deadline.is_some_and(|deadline| deadline <= now) {
            return None;
        }
        Some(value.value.clone())
    }

    fn visible_value(&self, key: &str, tx: Option<u64>, now: u64) -> Option<String> {
        if let Some(id) = tx {
            if let Some(value) = self.transactions.get(&id)?.writes.get(key) {
                return Self::live_value(value.as_ref(), now);
            }
        }

        let version = self.selected_version(key, tx)?;
        Self::live_value(version.value.as_ref(), now)
    }

    fn visible_keys(&self, tx: Option<u64>) -> Vec<String> {
        let mut keys = self.data.keys().cloned().collect::<Vec<_>>();
        if let Some(id) = tx {
            if let Some(transaction) = self.transactions.get(&id) {
                keys.extend(transaction.writes.keys().cloned());
            }
        }
        keys.sort();
        keys.dedup();
        keys
    }

    fn append_committed(&mut self, key: String, value: Option<Value>) {
        self.revision += 1;
        self.data.entry(key).or_default().push(Version {
            revision: self.revision,
            value,
        });
    }

    fn name_error(name: &str) -> Option<Error> {
        if name.is_empty() {
            return Some(Error::NameEmpty);
        }
        for (at, ch) in name.char_indices() {
            if !(ch.is_ascii_lowercase() || ch.is_ascii_digit() || ch == '_')
                || (at == 0 && !ch.is_ascii_lowercase())
            {
                return Some(Error::NameInvalid { at });
            }
        }
        None
    }

    fn range_matches(key: &str, start: &str, end: Option<&str>, prefix: Option<&str>) -> bool {
        key >= start
            && end.is_none_or(|end| key < end)
            && prefix.is_none_or(|prefix| key.starts_with(prefix))
    }

    fn visible_entries(
        &self,
        tx: Option<u64>,
        now: u64,
        start: &str,
        end: Option<&str>,
        prefix: Option<&str>,
    ) -> Vec<(String, String)> {
        self.visible_keys(tx)
            .into_iter()
            .filter(|key| Self::range_matches(key, start, end, prefix))
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

        if transaction.writes.is_empty() {
            self.transactions.remove(&id);
            return Ok(self.revision);
        }

        let begin_revision = transaction.begin_revision;
        let writes = transaction
            .writes
            .iter()
            .map(|(key, value)| (key.clone(), value.clone()))
            .collect::<Vec<_>>();

        for (key, _) in &writes {
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

        self.revision += 1;
        let revision = self.revision;
        for (key, value) in writes {
            self.data.entry(key).or_default().push(Version {
                revision,
                value,
            });
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
        if key.is_empty() {
            return Err(Error::KeyEmpty);
        }
        if let Some(id) = tx {
            if self.transactions.get(&id).unwrap().read_only {
                return Err(Error::ReadOnly(id));
            }
        }

        let stored = match ttl {
            Some(0) => None,
            Some(duration) => Some(Value {
                value: value.to_string(),
                deadline: Some(now.checked_add(duration).ok_or(Error::DeadlineOverflow)?),
            }),
            None => Some(Value {
                value: value.to_string(),
                deadline: None,
            }),
        };

        if let Some(id) = tx {
            self.transactions
                .get_mut(&id)
                .unwrap()
                .writes
                .insert(key.to_string(), stored);
        } else {
            self.append_committed(key.to_string(), stored);
        }
        Ok(())
    }

    pub fn delete(&mut self, now: u64, tx: Option<u64>, key: &str) -> Result<bool, Error> {
        self.admit(now)?;
        self.validate_tx(tx)?;
        if key.is_empty() {
            return Err(Error::KeyEmpty);
        }
        if let Some(id) = tx {
            if self.transactions.get(&id).unwrap().read_only {
                return Err(Error::ReadOnly(id));
            }
        }

        let existed = self.visible_value(key, tx, now).is_some();
        if let Some(id) = tx {
            self.transactions
                .get_mut(&id)
                .unwrap()
                .writes
                .insert(key.to_string(), None);
        } else {
            self.append_committed(key.to_string(), None);
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
        if key.is_empty() {
            return Err(Error::KeyEmpty);
        }
        Ok(self.visible_value(key, tx, now))
    }

    pub fn count(&mut self, now: u64, tx: Option<u64>, value: &str) -> Result<usize, Error> {
        self.admit(now)?;
        self.validate_tx(tx)?;
        let mut count = 0;
        for key in self.visible_keys(tx) {
            if self
                .visible_value(&key, tx, now)
                .is_some_and(|visible| visible == value)
            {
                count += 1;
            }
        }
        Ok(count)
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
        if end.is_some_and(|end| end <= start) {
            return Err(Error::InvalidRange);
        }
        if let Some(cursor) = cursor {
            if cursor.start != start
                || cursor.end.as_deref() != end
                || cursor.prefix.as_deref() != prefix
                || cursor.offset != offset
                || cursor.last.is_empty()
                || !Self::range_matches(&cursor.last, start, end, prefix)
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

        let mut candidates = self
            .visible_entries(tx, now, start, end, prefix)
            .into_iter()
            .filter(|(key, _)| {
                cursor.is_none_or(|cursor| key.as_str() > cursor.last.as_str())
            })
            .collect::<Vec<_>>();

        if cursor.is_none() {
            let skip = offset.min(candidates.len());
            candidates.drain(..skip);
        }

        let take = limit.min(candidates.len());
        let has_more = take < candidates.len();
        let items = candidates.into_iter().take(take).collect::<Vec<_>>();
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
        self.validate_tx(tx)?;
        if end.is_some_and(|end| end <= start) {
            return Err(Error::InvalidRange);
        }

        let entries = self.visible_entries(tx, now, start, end, prefix);
        let mut count = 0usize;
        let mut sum = 0i64;

        for (key, value) in entries {
            let parsed = Self::parse_canonical_i64(&value).ok_or_else(|| {
                Error::InvalidNumber {
                    key: key.clone(),
                    value: value.clone(),
                }
            })?;
            sum = sum
                .checked_add(parsed)
                .ok_or_else(|| Error::SumOverflow { key: key.clone() })?;
            count += 1;
        }

        Ok(Aggregate { count, sum })
    }

    fn parse_canonical_i64(value: &str) -> Option<i64> {
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
            return if value == "0" { Some(0) } else { None };
        }
        if digits.starts_with('0') || !digits.bytes().all(|byte| byte.is_ascii_digit()) {
            return None;
        }
        value.parse::<i64>().ok()
    }

    pub fn snapshot(&mut self, now: u64, name: &str) -> Result<(), Error> {
        self.admit(now)?;
        if let Some(error) = Self::name_error(name) {
            return Err(error);
        }
        if !self.snapshots.contains_key(name) && self.snapshots.len() >= 3 {
            return Err(Error::SnapshotLimit);
        }

        let mut saved = BTreeMap::new();
        for (key, versions) in &self.data {
            if let Some(value) = versions
                .last()
                .and_then(|version| version.value.as_ref())
                .filter(|value| value.deadline.is_none_or(|deadline| deadline > now))
            {
                saved.insert(key.clone(), value.clone());
            }
        }
        self.snapshots.insert(name.to_string(), saved);
        Ok(())
    }

    pub fn restore(&mut self, now: u64, name: &str) -> Result<u64, Error> {
        self.admit(now)?;
        if let Some(error) = Self::name_error(name) {
            return Err(error);
        }
        let saved = self
            .snapshots
            .get(name)
            .cloned()
            .ok_or_else(|| Error::SnapshotMissing(name.to_string()))?;
        if !self.transactions.is_empty() {
            return Err(Error::ActiveTransactions);
        }

        let copied_live = saved
            .into_iter()
            .filter(|(_, value)| value.deadline.is_none_or(|deadline| deadline > now))
            .collect::<BTreeMap<_, _>>();

        let mut union = BTreeMap::<String, Option<Value>>::new();
        for (key, versions) in &self.data {
            if versions
                .last()
                .and_then(|version| version.value.as_ref())
                .is_some_and(|value| value.deadline.is_none_or(|deadline| deadline > now))
            {
                union.insert(key.clone(), None);
            }
        }
        for (key, value) in copied_live {
            union.insert(key, Some(value));
        }

        if union.is_empty() {
            return Ok(self.revision);
        }

        self.revision += 1;
        let revision = self.revision;
        for (key, value) in union {
            self.data
                .entry(key)
                .or_default()
                .push(Version { revision, value });
        }

        Ok(revision)
    }

    pub fn forget(&mut self, now: u64, name: &str) -> Result<(), Error> {
        self.admit(now)?;
        if let Some(error) = Self::name_error(name) {
            return Err(error);
        }
        if self.snapshots.remove(name).is_none() {
            return Err(Error::SnapshotMissing(name.to_string()));
        }
        Ok(())
    }

    pub fn gc(&mut self, now: u64) -> Result<usize, Error> {
        self.admit(now)?;
        let mut removed = 0usize;

        for versions in self.data.values_mut() {
            if versions.len() <= 1 {
                continue;
            }

            let mut keep = vec![false; versions.len()];
            keep[versions.len() - 1] = true;

            for transaction in self.transactions.values() {
                if let Some(index) = versions
                    .iter()
                    .rposition(|version| version.revision <= transaction.begin_revision)
                {
                    keep[index] = true;
                }
            }

            let mut index = 0usize;
            let old_len = versions.len();
            versions.retain(|_| {
                let retain = keep[index];
                index += 1;
                retain
            });
            removed += old_len - versions.len();
        }

        Ok(removed)
    }

    pub fn stats(&mut self, now: u64) -> Result<Stats, Error> {
        self.admit(now)?;
        let keys = self.data.len();
        let mut live_keys = 0usize;
        let mut versions = 0usize;
        let mut tombstones = 0usize;

        for history in self.data.values() {
            versions += history.len();
            tombstones += history
                .iter()
                .filter(|version| version.value.is_none())
                .count();
            if history
                .last()
                .and_then(|version| version.value.as_ref())
                .is_some_and(|value| value.deadline.is_none_or(|deadline| deadline > now))
            {
                live_keys += 1;
            }
        }

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