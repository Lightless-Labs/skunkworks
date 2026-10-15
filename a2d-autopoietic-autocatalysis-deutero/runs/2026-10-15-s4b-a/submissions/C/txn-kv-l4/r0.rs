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
struct Entry {
    value: Option<String>,
    deadline: Option<u64>,
}

#[derive(Clone)]
struct Version {
    revision: u64,
    entry: Entry,
}

struct Transaction {
    begin_revision: u64,
    read_only: bool,
    writes: BTreeMap<String, Entry>,
}

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

    fn validate_tx(&self, tx: Option<u64>) -> Result<(), Error> {
        if let Some(id) = tx {
            if !self.transactions.contains_key(&id) {
                return Err(Error::UnknownTransaction(id));
            }
        }
        Ok(())
    }

    fn live(entry: &Entry, now: u64) -> bool {
        entry.value.is_some() && entry.deadline.map_or(true, |deadline| deadline > now)
    }

    fn visible_entry(&self, tx: Option<u64>, key: &str) -> Option<Entry> {
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

    fn visible_live(&self, tx: Option<u64>, key: &str, now: u64) -> Option<String> {
        let entry = self.visible_entry(tx, key)?;
        if Self::live(&entry, now) {
            entry.value
        } else {
            None
        }
    }

    fn matching_keys(
        &self,
        tx: Option<u64>,
        now: u64,
        start: &str,
        end: Option<&str>,
        prefix: Option<&str>,
    ) -> Vec<(String, String)> {
        let mut out = Vec::new();
        for key in self.data.keys() {
            if key.as_str() < start
                || end.map_or(false, |end| key.as_str() >= end)
                || prefix.map_or(false, |prefix| !key.starts_with(prefix))
            {
                continue;
            }
            if let Some(value) = self.visible_live(tx, key, now) {
                out.push((key.clone(), value));
            }
        }

        if let Some(id) = tx {
            if let Some(transaction) = self.transactions.get(&id) {
                for (key, entry) in &transaction.writes {
                    if self.data.contains_key(key)
                        || key.as_str() < start
                        || end.map_or(false, |end| key.as_str() >= end)
                        || prefix.map_or(false, |prefix| !key.starts_with(prefix))
                    {
                        continue;
                    }
                    if Self::live(entry, now) {
                        out.push((key.clone(), entry.value.clone().unwrap()));
                    }
                }
            }
        }

        out.sort_by(|a, b| a.0.cmp(&b.0));
        out
    }

    fn append_committed(&mut self, key: String, entry: Entry) {
        self.revision += 1;
        self.data.entry(key).or_default().push(Version {
            revision: self.revision,
            entry,
        });
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
                if self
                    .data
                    .get(key)
                    .and_then(|versions| versions.last())
                    .map_or(false, |version| version.revision > transaction.begin_revision)
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
            for (key, entry) in writes {
                self.data
                    .entry(key)
                    .or_default()
                    .push(Version { revision, entry });
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
        self.validate_tx(tx)?;
        if key.is_empty() {
            return Err(Error::KeyEmpty);
        }
        if let Some(id) = tx {
            if self.transactions[&id].read_only {
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
                deadline: Some(
                    now.checked_add(duration)
                        .ok_or(Error::DeadlineOverflow)?,
                ),
            },
            None => Entry {
                value: Some(value.to_string()),
                deadline: None,
            },
        };

        if let Some(id) = tx {
            self.transactions
                .get_mut(&id)
                .unwrap()
                .writes
                .insert(key.to_string(), entry);
        } else {
            self.append_committed(key.to_string(), entry);
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
            if self.transactions[&id].read_only {
                return Err(Error::ReadOnly(id));
            }
        }

        let existed = self.visible_live(tx, key, now).is_some();
        let tombstone = Entry {
            value: None,
            deadline: None,
        };
        if let Some(id) = tx {
            self.transactions
                .get_mut(&id)
                .unwrap()
                .writes
                .insert(key.to_string(), tombstone);
        } else {
            self.append_committed(key.to_string(), tombstone);
        }
        Ok(existed)
    }

    pub fn get(&mut self, now: u64, tx: Option<u64>, key: &str) -> Result<Option<String>, Error> {
        self.admit(now)?;
        self.validate_tx(tx)?;
        if key.is_empty() {
            return Err(Error::KeyEmpty);
        }
        Ok(self.visible_live(tx, key, now))
    }

    pub fn count(&mut self, now: u64, tx: Option<u64>, value: &str) -> Result<usize, Error> {
        self.admit(now)?;
        self.validate_tx(tx)?;
        Ok(self
            .matching_keys(tx, now, "", None, None)
            .iter()
            .filter(|(_, item_value)| item_value == value)
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
        if end.map_or(false, |end| end <= start) {
            return Err(Error::InvalidRange);
        }
        if let Some(cursor) = cursor {
            if cursor.start != start
                || cursor.end.as_deref() != end
                || cursor.prefix.as_deref() != prefix
                || cursor.offset != offset
                || cursor.last.is_empty()
                || cursor.last.as_str() < start
                || end.map_or(false, |end| cursor.last.as_str() >= end)
                || prefix.map_or(false, |prefix| !cursor.last.starts_with(prefix))
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

        let mut all = self.matching_keys(tx, now, start, end, prefix);
        if let Some(cursor) = cursor {
            all.retain(|(key, _)| key.as_str() > cursor.last.as_str());
        } else if offset >= all.len() {
            return Ok(Page {
                items: Vec::new(),
                next: None,
            });
        } else {
            all.drain(..offset);
        }

        let take = limit.min(all.len());
        let has_more = all.len() > take;
        let items: Vec<_> = all.into_iter().take(take).collect();
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
        if end.map_or(false, |end| end <= start) {
            return Err(Error::InvalidRange);
        }

        let entries = self.matching_keys(tx, now, start, end, prefix);
        let mut sum: i64 = 0;
        for (key, value) in &entries {
            let bytes = value.as_bytes();
            let valid = if value == "0" {
                true
            } else {
                let digits = if bytes.first() == Some(&b'-') {
                    &bytes[1..]
                } else {
                    bytes
                };
                !digits.is_empty()
                    && (b'1'..=b'9').contains(&digits[0])
                    && digits.iter().all(|byte| byte.is_ascii_digit())
                    && !(bytes.first() == Some(&b'-') && digits == b"0")
                    && !(digits.len() > 1 && digits[0] == b'0')
            };
            if !valid {
                return Err(Error::InvalidNumber {
                    key: key.clone(),
                    value: value.clone(),
                });
            }

            let number: i64 = value.parse().map_err(|_| Error::InvalidNumber {
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
        if name.is_empty() {
            return Err(Error::NameEmpty);
        }
        let bytes = name.as_bytes();
        if !bytes[0].is_ascii_lowercase() {
            return Err(Error::NameInvalid { at: 0 });
        }
        for (index, byte) in bytes.iter().enumerate().skip(1) {
            if !(byte.is_ascii_lowercase() || byte.is_ascii_digit() || *byte == b'_') {
                return Err(Error::NameInvalid { at: index });
            }
        }
        if !self.snapshots.contains_key(name) && self.snapshots.len() >= 3 {
            return Err(Error::SnapshotLimit);
        }

        let mut saved = BTreeMap::new();
        for (key, versions) in &self.data {
            if let Some(version) = versions.last() {
                if Self::live(&version.entry, now) {
                    saved.insert(key.clone(), version.entry.clone());
                }
            }
        }
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

        let saved: BTreeMap<String, Entry> = saved
            .into_iter()
            .filter(|(_, entry)| Self::live(entry, now))
            .collect();
        let mut union = BTreeSet::new();
        for (key, versions) in &self.data {
            if versions
                .last()
                .map_or(false, |version| Self::live(&version.entry, now))
            {
                union.insert(key.clone());
            }
        }
        union.extend(saved.keys().cloned());

        if union.is_empty() {
            return Ok(self.revision);
        }

        self.revision += 1;
        let revision = self.revision;
        for key in union {
            let entry = saved.get(&key).cloned().unwrap_or(Entry {
                value: None,
                deadline: None,
            });
            self.data
                .entry(key)
                .or_default()
                .push(Version { revision, entry });
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
        let begins: Vec<u64> = self
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
        let versions: usize = self.data.values().map(Vec::len).sum();
        let tombstones: usize = self
            .data
            .values()
            .flat_map(|versions| versions.iter())
            .filter(|version| version.entry.value.is_none())
            .count();
        let live_keys = self
            .data
            .iter()
            .filter(|(_, versions)| {
                versions
                    .last()
                    .map_or(false, |version| Self::live(&version.entry, now))
            })
            .count();

        Ok(Stats {
            clock: self.clock,
            revision: self.revision,
            keys: self.data.len(),
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn transaction_isolation_and_commit() {
        let mut store = Store::new();
        store.set(1, None, "key", "base", None).unwrap();
        store.begin(1, 7, false).unwrap();
        store.set(2, None, "key", "later", None).unwrap();
        assert_eq!(store.get(2, Some(7), "key"), Ok(Some("base".into())));
        store.set(2, Some(7), "other", "own", None).unwrap();
        assert_eq!(store.count(2, Some(7), "own"), Ok(1));
        assert_eq!(store.commit(2, 7), Ok(3));
    }

    #[test]
    fn error_precedence_and_backwards_time() {
        let mut store = Store::new();
        assert_eq!(
            store.set(1, Some(9), "", "x", Some(u64::MAX)),
            Err(Error::UnknownTransaction(9))
        );
        store.begin(1, 2, true).unwrap();
        assert_eq!(
            store.set(1, Some(2), "", "x", Some(u64::MAX)),
            Err(Error::KeyEmpty)
        );
        assert_eq!(
            store.set(1, Some(2), "x", "x", Some(u64::MAX)),
            Err(Error::ReadOnly(2))
        );
        assert_eq!(
            store.get(0, None, "x"),
            Err(Error::TimeBackwards {
                previous: 1,
                now: 0
            })
        );
    }

    #[test]
    fn cursor_resumes_without_reapplying_offset() {
        let mut store = Store::new();
        for key in ["a", "b", "c", "d"] {
            store.set(0, None, key, key, None).unwrap();
        }
        let first = store.scan(0, None, "", None, None, 1, 1, None).unwrap();
        assert_eq!(first.items, vec![("b".into(), "b".into())]);
        let second = store
            .scan(0, None, "", None, None, 1, 1, first.next.as_ref())
            .unwrap();
        assert_eq!(second.items, vec![("c".into(), "c".into())]);
    }

    #[test]
    fn aggregate_validates_and_overflows_in_key_order() {
        let mut store = Store::new();
        store.set(0, None, "a", "-0", None).unwrap();
        store.set(0, None, "b", "1", None).unwrap();
        assert_eq!(
            store.aggregate(0, None, "", None, None),
            Err(Error::InvalidNumber {
                key: "a".into(),
                value: "-0".into()
            })
        );

        let mut store = Store::new();
        store.set(0, None, "a", i64::MAX.to_string().as_str(), None).unwrap();
        store.set(0, None, "b", "1", None).unwrap();
        assert_eq!(
            store.aggregate(0, None, "", None, None),
            Err(Error::SumOverflow { key: "b".into() })
        );
    }

    #[test]
    fn snapshots_gc_and_stats() {
        let mut store = Store::new();
        store.set(1, None, "x", "old", Some(20)).unwrap();
        store.snapshot(1, "saved").unwrap();
        store.set(2, None, "x", "new", None).unwrap();
        assert_eq!(store.restore(2, "saved"), Ok(3));
        assert_eq!(store.get(2, None, "x"), Ok(Some("old".into())));
        assert_eq!(store.delete(2, None, "x"), Ok(true));
        assert_eq!(store.gc(2), Ok(3));
        let stats = store.stats(2).unwrap();
        assert_eq!(
            (stats.revision, stats.versions, stats.tombstones, stats.live_keys),
            (4, 1, 1, 0)
        );
        store.forget(2, "saved").unwrap();
    }
}

fn main() {}