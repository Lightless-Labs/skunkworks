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
#[derive(Clone)]
struct Entry {
    value: String,
    deadline: Option<u64>,
}
type Map = BTreeMap<String, Entry>;
struct Version {
    revision: u64,
    entry: Option<Entry>,
}
struct Transaction {
    start: u64,
    read_only: bool,
    writes: BTreeMap<String, Option<Entry>>,
}
pub struct Store {
    clock: u64,
    revision: u64,
    versions: BTreeMap<String, Vec<Version>>,
    transactions: BTreeMap<u64, Transaction>,
    snapshots: BTreeMap<String, Map>,
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
    fn tx(&self, tx: Option<u64>) -> Result<Option<&Transaction>, Error> {
        tx.map(|id| {
            self.transactions
                .get(&id)
                .ok_or(Error::UnknownTransaction(id))
        })
        .transpose()
    }
    fn key(key: &str) -> Result<(), Error> {
        if key.is_empty() {
            Err(Error::KeyEmpty)
        } else {
            Ok(())
        }
    }
    fn name(name: &str) -> Result<(), Error> {
        if name.is_empty() {
            return Err(Error::NameEmpty);
        }
        for (at, c) in name.char_indices() {
            if !(c.is_ascii_lowercase() || (at > 0 && (c.is_ascii_digit() || c == '_'))) {
                return Err(Error::NameInvalid { at });
            }
        }
        Ok(())
    }
    fn writable(&self, tx: Option<u64>) -> Result<(), Error> {
        if let Some(id) = tx {
            if self.transactions[&id].read_only {
                return Err(Error::ReadOnly(id));
            }
        }
        Ok(())
    }
    fn live(entry: &Entry, now: u64) -> bool {
        entry.deadline.is_none_or(|d| d > now)
    }
    fn view(&self, tx: Option<u64>, now: u64) -> Result<Map, Error> {
        let transaction = self.tx(tx)?;
        let revision = transaction.map_or(self.revision, |t| t.start);
        let mut map = Map::new();
        for (key, versions) in &self.versions {
            if let Some(version) = versions.iter().rev().find(|v| v.revision <= revision) {
                if let Some(e) = &version.entry {
                    if Self::live(e, now) {
                        map.insert(key.clone(), e.clone());
                    }
                }
            }
        }
        if let Some(t) = transaction {
            for (key, entry) in &t.writes {
                map.remove(key);
                if let Some(e) = entry {
                    if Self::live(e, now) {
                        map.insert(key.clone(), e.clone());
                    }
                }
            }
        }
        Ok(map)
    }
    fn append(&mut self, key: String, entry: Option<Entry>) {
        self.versions.entry(key).or_default().push(Version {
            revision: self.revision,
            entry,
        });
    }
    fn write(&mut self, tx: Option<u64>, key: &str, entry: Option<Entry>) {
        if let Some(id) = tx {
            self.transactions
                .get_mut(&id)
                .unwrap()
                .writes
                .insert(key.into(), entry);
        } else {
            self.revision += 1;
            self.append(key.into(), entry);
        }
    }
    pub fn begin(&mut self, now: u64, id: u64, read_only: bool) -> Result<(), Error> {
        self.admit(now)?;
        if self.transactions.contains_key(&id) {
            return Err(Error::TransactionExists(id));
        }
        self.transactions.insert(
            id,
            Transaction {
                start: self.revision,
                read_only,
                writes: BTreeMap::new(),
            },
        );
        Ok(())
    }
    pub fn commit(&mut self, now: u64, id: u64) -> Result<u64, Error> {
        self.admit(now)?;
        let t = self
            .transactions
            .get(&id)
            .ok_or(Error::UnknownTransaction(id))?;
        for key in t.writes.keys() {
            if self
                .versions
                .get(key)
                .and_then(|v| v.last())
                .is_some_and(|v| v.revision > t.start)
            {
                return Err(Error::Conflict {
                    tx: id,
                    key: key.clone(),
                });
            }
        }
        let t = self.transactions.remove(&id).unwrap();
        if !t.writes.is_empty() {
            self.revision += 1;
            for (key, entry) in t.writes {
                self.append(key, entry);
            }
        }
        Ok(self.revision)
    }
    pub fn rollback(&mut self, now: u64, id: u64) -> Result<(), Error> {
        self.admit(now)?;
        self.transactions
            .remove(&id)
            .ok_or(Error::UnknownTransaction(id))?;
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
        self.tx(tx)?;
        Self::key(key)?;
        self.writable(tx)?;
        let deadline = match ttl {
            None => None,
            Some(d) => Some(now.checked_add(d).ok_or(Error::DeadlineOverflow)?),
        };
        let entry = if ttl == Some(0) {
            None
        } else {
            Some(Entry {
                value: value.into(),
                deadline,
            })
        };
        self.write(tx, key, entry);
        Ok(())
    }
    pub fn delete(&mut self, now: u64, tx: Option<u64>, key: &str) -> Result<bool, Error> {
        self.admit(now)?;
        self.tx(tx)?;
        Self::key(key)?;
        self.writable(tx)?;
        let present = self.view(tx, now)?.contains_key(key);
        self.write(tx, key, None);
        Ok(present)
    }
    pub fn get(&mut self, now: u64, tx: Option<u64>, key: &str) -> Result<Option<String>, Error> {
        self.admit(now)?;
        self.tx(tx)?;
        Self::key(key)?;
        Ok(self.view(tx, now)?.get(key).map(|e| e.value.clone()))
    }
    pub fn count(&mut self, now: u64, tx: Option<u64>, value: &str) -> Result<usize, Error> {
        self.admit(now)?;
        Ok(self
            .view(tx, now)?
            .values()
            .filter(|e| e.value == value)
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
        page(
            &self.view(tx, now)?,
            start,
            end,
            prefix,
            offset,
            limit,
            cursor,
        )
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
        total(&self.view(tx, now)?, start, end, prefix)
    }
    pub fn snapshot(&mut self, now: u64, name: &str) -> Result<(), Error> {
        self.admit(now)?;
        Self::name(name)?;
        if !self.snapshots.contains_key(name) && self.snapshots.len() == 3 {
            return Err(Error::SnapshotLimit);
        }
        self.snapshots.insert(name.into(), self.view(None, now)?);
        Ok(())
    }
    pub fn restore(&mut self, now: u64, name: &str) -> Result<u64, Error> {
        self.admit(now)?;
        Self::name(name)?;
        if !self.transactions.is_empty() {
            return Err(Error::ActiveTransactions);
        }
        let mut saved = self
            .snapshots
            .get(name)
            .ok_or_else(|| Error::SnapshotMissing(name.into()))?
            .clone();
        saved.retain(|_, e| Self::live(e, now));
        let current = self.view(None, now)?;
        let keys: BTreeSet<_> = current.keys().chain(saved.keys()).cloned().collect();
        if !keys.is_empty() {
            self.revision += 1;
            for key in keys {
                let entry = saved.get(&key).cloned();
                self.append(key, entry);
            }
        }
        Ok(self.revision)
    }
    pub fn forget(&mut self, now: u64, name: &str) -> Result<(), Error> {
        self.admit(now)?;
        Self::name(name)?;
        self.snapshots
            .remove(name)
            .ok_or_else(|| Error::SnapshotMissing(name.into()))?;
        Ok(())
    }
    pub fn gc(&mut self, now: u64) -> Result<usize, Error> {
        self.admit(now)?;
        let mut removed = 0;
        for versions in self.versions.values_mut() {
            let mut keep = BTreeSet::new();
            if !versions.is_empty() {
                keep.insert(versions.len() - 1);
            }
            for t in self.transactions.values() {
                if let Some(i) = versions.iter().rposition(|v| v.revision <= t.start) {
                    keep.insert(i);
                }
            }
            let mut index = 0;
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
        Ok(Stats {
            clock: self.clock,
            revision: self.revision,
            keys: self.versions.len(),
            live_keys: self.view(None, now)?.len(),
            versions: self.versions.values().map(Vec::len).sum(),
            tombstones: self
                .versions
                .values()
                .flatten()
                .filter(|v| v.entry.is_none())
                .count(),
            active_transactions: self.transactions.len(),
            read_only_transactions: self.transactions.values().filter(|t| t.read_only).count(),
            staged_writes: self.transactions.values().map(|t| t.writes.len()).sum(),
            snapshots: self.snapshots.len(),
        })
    }
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
fn range_ok(start: &str, end: Option<&str>) -> Result<(), Error> {
    if end.is_some_and(|e| e <= start) {
        Err(Error::InvalidRange)
    } else {
        Ok(())
    }
}
fn matches(key: &str, start: &str, end: Option<&str>, prefix: Option<&str>) -> bool {
    key >= start && end.is_none_or(|e| key < e) && prefix.is_none_or(|p| key.starts_with(p))
}
fn page(
    map: &Map,
    start: &str,
    end: Option<&str>,
    prefix: Option<&str>,
    offset: usize,
    limit: usize,
    cursor: Option<&Cursor>,
) -> Result<Page, Error> {
    range_ok(start, end)?;
    if let Some(c) = cursor {
        if c.start != start
            || c.end.as_deref() != end
            || c.prefix.as_deref() != prefix
            || c.offset != offset
            || c.last.is_empty()
            || !matches(&c.last, start, end, prefix)
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
    let skip = if cursor.is_some() { 0 } else { offset };
    let mut iter = map
        .iter()
        .filter(|(k, _)| {
            matches(k, start, end, prefix) && cursor.is_none_or(|c| k.as_str() > c.last.as_str())
        })
        .skip(skip);
    let items: Vec<_> = iter
        .by_ref()
        .take(limit)
        .map(|(k, e)| (k.clone(), e.value.clone()))
        .collect();
    let next = if !items.is_empty() && iter.next().is_some() {
        Some(Cursor {
            start: start.into(),
            end: end.map(str::to_string),
            prefix: prefix.map(str::to_string),
            offset,
            last: items.last().unwrap().0.clone(),
        })
    } else {
        None
    };
    Ok(Page { items, next })
}
fn number(value: &str) -> Option<i64> {
    let bytes = value.as_bytes();
    if value == "0" {
        return Some(0);
    }
    let digits = if bytes.first() == Some(&b'-') {
        &bytes[1..]
    } else {
        bytes
    };
    if digits.is_empty()
        || !(b'1'..=b'9').contains(&digits[0])
        || !digits.iter().all(u8::is_ascii_digit)
    {
        return None;
    }
    value.parse().ok()
}
fn total(
    map: &Map,
    start: &str,
    end: Option<&str>,
    prefix: Option<&str>,
) -> Result<Aggregate, Error> {
    range_ok(start, end)?;
    let mut out = Aggregate { count: 0, sum: 0 };
    for (key, e) in map.iter().filter(|(k, _)| matches(k, start, end, prefix)) {
        let n = number(&e.value).ok_or_else(|| Error::InvalidNumber {
            key: key.clone(),
            value: e.value.clone(),
        })?;
        out.sum = out
            .sum
            .checked_add(n)
            .ok_or_else(|| Error::SumOverflow { key: key.clone() })?;
        out.count += 1;
    }
    Ok(out)
}
