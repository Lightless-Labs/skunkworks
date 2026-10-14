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
struct Value {
    value: String,
    deadline: Option<u64>,
}

#[derive(Clone)]
struct Version {
    revision: u64,
    value: Option<Value>,
}

#[derive(Clone)]
struct Write {
    value: Option<Value>,
}

struct Transaction {
    begin_revision: u64,
    read_only: bool,
    writes: BTreeMap<String, Write>,
}

pub struct Store {
    clock: u64,
    revision: u64,
    data: BTreeMap<String, Vec<Version>>,
    transactions: BTreeMap<u64, Transaction>,
    snapshots: BTreeMap<String, BTreeMap<String, Value>>,
}

impl Store {
    /// Creates an empty store with clock and revision set to zero.
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

    fn selected_version(&self, key: &str, tx: Option<u64>) -> Option<&Version> {
        let versions = self.data.get(key)?;
        let max_revision = tx
            .and_then(|id| self.transactions.get(&id).map(|t| t.begin_revision))
            .unwrap_or(u64::MAX);
        versions
            .iter()
            .rev()
            .find(|v| v.revision <= max_revision)
    }

    fn visible_value(&self, key: &str, tx: Option<u64>, now: u64) -> Option<Value> {
        if let Some(id) = tx {
            if let Some(write) = self.transactions.get(&id)?.writes.get(key) {
                return write
                    .value
                    .as_ref()
                    .filter(|v| v.deadline.is_none_or(|d| d > now))
                    .cloned();
            }
        }
        self.selected_version(key, tx)
            .and_then(|v| v.value.as_ref())
            .filter(|v| v.deadline.is_none_or(|d| d > now))
            .cloned()
    }

    fn live_keys(&self, tx: Option<u64>, now: u64) -> BTreeMap<String, Value> {
        let mut keys = BTreeSet::new();
        keys.extend(self.data.keys().cloned());
        if let Some(id) = tx {
            if let Some(t) = self.transactions.get(&id) {
                keys.extend(t.writes.keys().cloned());
            }
        }
        keys.into_iter()
            .filter_map(|k| self.visible_value(&k, tx, now).map(|v| (k, v)))
            .collect()
    }

    fn append(&mut self, key: String, value: Option<Value>) {
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
        let mut bytes = name.bytes();
        let first = bytes.next().unwrap();
        if !first.is_ascii_lowercase() {
            return Some(Error::NameInvalid { at: 0 });
        }
        for (i, b) in name.bytes().enumerate().skip(1) {
            if !(b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'_') {
                return Some(Error::NameInvalid { at: i });
            }
        }
        None
    }

    fn range_keys(
        &self,
        tx: Option<u64>,
        now: u64,
        start: &str,
        end: Option<&str>,
        prefix: Option<&str>,
    ) -> BTreeMap<String, Value> {
        self.live_keys(tx, now)
            .into_iter()
            .filter(|(k, _)| {
                k.as_str() >= start
                    && end.is_none_or(|e| k.as_str() < e)
                    && prefix.is_none_or(|p| k.starts_with(p))
            })
            .collect()
    }

    /// Starts a transaction at the current committed revision.
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

    /// Commits a transaction atomically, or reports its first write conflict.
    pub fn commit(&mut self, now: u64, id: u64) -> Result<u64, Error> {
        self.admit(now)?;
        let Some(tx) = self.transactions.get(&id) else {
            return Err(Error::UnknownTransaction(id));
        };
        if tx.writes.is_empty() {
            self.transactions.remove(&id);
            return Ok(self.revision);
        }
        let begin_revision = tx.begin_revision;
        let writes = tx
            .writes
            .iter()
            .map(|(k, w)| (k.clone(), w.value.clone()))
            .collect::<Vec<_>>();
        for (key, _) in &writes {
            if self
                .data
                .get(key)
                .and_then(|vs| vs.last())
                .is_some_and(|v| v.revision > begin_revision)
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
            self.data.entry(key).or_default().push(Version { revision, value });
        }
        self.transactions.remove(&id);
        Ok(revision)
    }

    /// Rolls back a transaction and discards its staged writes.
    pub fn rollback(&mut self, now: u64, id: u64) -> Result<(), Error> {
        self.admit(now)?;
        if self.transactions.remove(&id).is_none() {
            return Err(Error::UnknownTransaction(id));
        }
        Ok(())
    }

    /// Sets a key permanently or until its optional TTL expires.
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
            if self.transactions.get(&id).unwrap().read_only {
                return Err(Error::ReadOnly(id));
            }
        }
        let deadline = match ttl {
            Some(d) => Some(now.checked_add(d).ok_or(Error::DeadlineOverflow)?),
            None => None,
        };
        let val = if deadline == Some(now) {
            None
        } else {
            Some(Value {
                value: value.to_string(),
                deadline,
            })
        };
        if let Some(id) = tx {
            self.transactions
                .get_mut(&id)
                .unwrap()
                .writes
                .insert(key.to_string(), Write { value: val });
        } else {
            self.append(key.to_string(), val);
        }
        Ok(())
    }

    /// Deletes a key and reports whether it was live before deletion.
    pub fn delete(&mut self, now: u64, tx: Option<u64>, key: &str) -> Result<bool, Error> {
        self.admit(now)?;
        self.check_tx(tx)?;
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
                .insert(key.to_string(), Write { value: None });
        } else {
            self.append(key.to_string(), None);
        }
        Ok(existed)
    }

    /// Gets an owned live value for a valid key.
    pub fn get(&mut self, now: u64, tx: Option<u64>, key: &str) -> Result<Option<String>, Error> {
        self.admit(now)?;
        self.check_tx(tx)?;
        if key.is_empty() {
            return Err(Error::KeyEmpty);
        }
        Ok(self.visible_value(key, tx, now).map(|v| v.value))
    }

    /// Counts live visible keys whose value exactly matches the argument.
    pub fn count(&mut self, now: u64, tx: Option<u64>, value: &str) -> Result<usize, Error> {
        self.admit(now)?;
        self.check_tx(tx)?;
        Ok(self
            .live_keys(tx, now)
            .values()
            .filter(|v| v.value == value)
            .count())
    }

    /// Scans matching live keys, optionally resuming after a cursor.
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
        if end.is_some_and(|e| e <= start) {
            return Err(Error::InvalidRange);
        }
        if let Some(c) = cursor {
            if c.start != start
                || c.end.as_deref() != end
                || c.prefix.as_deref() != prefix
                || c.offset != offset
                || c.last.is_empty()
                || c.last.as_str() < start
                || end.is_some_and(|e| c.last.as_str() >= e)
                || prefix.is_some_and(|p| !c.last.starts_with(p))
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
        let all = self.range_keys(tx, now, start, end, prefix);
        let iter = all.into_iter().filter(|(k, _)| {
            if let Some(c) = cursor {
                k.as_str() > c.last.as_str()
            } else {
                true
            }
        });
        let skip = if cursor.is_none() { offset } else { 0 };
        let remaining = iter.skip(skip);
        let mut selected = remaining.take(limit.saturating_add(1)).collect::<Vec<_>>();
        let has_more = selected.len() > limit;
        if has_more {
            selected.pop();
        }
        let next = if has_more {
            selected.last().map(|(last, _)| Cursor {
                start: start.to_string(),
                end: end.map(str::to_string),
                prefix: prefix.map(str::to_string),
                offset,
                last: last.clone(),
            })
        } else {
            None
        };
        Ok(Page {
            items: selected
                .into_iter()
                .map(|(k, v)| (k, v.value))
                .collect(),
            next,
        })
    }

    /// Aggregates matching live values as canonical signed 64-bit integers.
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
        if end.is_some_and(|e| e <= start) {
            return Err(Error::InvalidRange);
        }
        let keys = self.range_keys(tx, now, start, end, prefix);
        let mut count = 0;
        let mut sum = 0i64;
        for (key, val) in keys {
            let value = val.value.as_str();
            let bytes = value.as_bytes();
            let digits = if bytes.first() == Some(&b'-') {
                &bytes[1..]
            } else {
                bytes
            };
            let canonical = if value == "0" {
                true
            } else if value == "-0" || digits.is_empty() || digits[0] == b'0' {
                false
            } else {
                digits.iter().all(u8::is_ascii_digit)
            };
            let parsed = if canonical {
                value.parse::<i64>().ok()
            } else {
                None
            };
            let Some(number) = parsed else {
                return Err(Error::InvalidNumber {
                    key,
                    value: val.value,
                });
            };
            sum = sum.checked_add(number).ok_or_else(|| Error::SumOverflow {
                key: key.clone(),
            })?;
            count += 1;
        }
        Ok(Aggregate { count, sum })
    }

    /// Saves a materialized copy of current committed live data.
    pub fn snapshot(&mut self, now: u64, name: &str) -> Result<(), Error> {
        self.admit(now)?;
        if let Some(e) = Self::name_error(name) {
            return Err(e);
        }
        if !self.snapshots.contains_key(name) && self.snapshots.len() >= 3 {
            return Err(Error::SnapshotLimit);
        }
        self.snapshots
            .insert(name.to_string(), self.live_keys(None, now));
        Ok(())
    }

    /// Restores a saved snapshot as one atomic committed revision.
    pub fn restore(&mut self, now: u64, name: &str) -> Result<u64, Error> {
        self.admit(now)?;
        if name.is_empty() {
            return Err(Error::NameEmpty);
        }
        let Some(saved) = self.snapshots.get(name) else {
            return Err(Error::SnapshotMissing(name.to_string()));
        };
        if !self.transactions.is_empty() {
            return Err(Error::ActiveTransactions);
        }
        let saved_live = saved
            .iter()
            .filter(|(_, v)| v.deadline.is_none_or(|d| d > now))
            .map(|(k, v)| (k.clone(), v.clone()))
            .collect::<BTreeMap<_, _>>();
        let current = self.live_keys(None, now);
        let union = current
            .keys()
            .chain(saved_live.keys())
            .cloned()
            .collect::<BTreeSet<_>>();
        if union.is_empty() {
            return Ok(self.revision);
        }
        self.revision += 1;
        let revision = self.revision;
        for key in union {
            let value = saved_live.get(&key).cloned();
            self.data
                .entry(key)
                .or_default()
                .push(Version { revision, value });
        }
        Ok(revision)
    }

    /// Removes a named snapshot.
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

    /// Removes committed versions not needed by the latest view or active transactions.
    pub fn gc(&mut self, now: u64) -> Result<usize, Error> {
        self.admit(now)?;
        let mut removed = 0;
        for versions in self.data.values_mut() {
            if versions.len() <= 1 {
                continue;
            }
            let mut keep = BTreeSet::new();
            keep.insert(versions.len() - 1);
            for tx in self.transactions.values() {
                if let Some((i, _)) = versions
                    .iter()
                    .enumerate()
                    .rev()
                    .find(|(_, v)| v.revision <= tx.begin_revision)
                {
                    keep.insert(i);
                }
            }
            let mut i = 0usize;
            versions.retain(|_| {
                let retain = keep.contains(&i);
                i += 1;
                if !retain {
                    removed += 1;
                }
                retain
            });
        }
        Ok(removed)
    }

    /// Returns counts for retained versions, live committed keys, and active work.
    pub fn stats(&mut self, now: u64) -> Result<Stats, Error> {
        self.admit(now)?;
        let versions = self.data.values().map(Vec::len).sum();
        let tombstones = self
            .data
            .values()
            .flat_map(|vs| vs.iter())
            .filter(|v| v.value.is_none())
            .count();
        let live_keys = self.live_keys(None, now).len();
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
                .filter(|t| t.read_only)
                .count(),
            staged_writes: self.transactions.values().map(|t| t.writes.len()).sum(),
            snapshots: self.snapshots.len(),
        })
    }
}

impl Default for Store {
    fn default() -> Self {
        Self::new()
    }
}

/// Program entry point.
pub fn main() {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normal_transaction_path() {
        let mut s = Store::new();
        s.begin(0, 1, false).unwrap();
        s.set(0, Some(1), "a", "1", None).unwrap();
        assert_eq!(s.commit(0, 1), Ok(1));
        assert_eq!(s.get(0, None, "a"), Ok(Some("1".into())));
    }

    #[test]
    fn invalid_key_and_time() {
        let mut s = Store::new();
        assert_eq!(s.get(1, None, ""), Err(Error::KeyEmpty));
        assert_eq!(
            s.get(0, None, "x"),
            Err(Error::TimeBackwards {
                previous: 1,
                now: 0
            })
        );
    }

    #[test]
    fn snapshot_round_trip() {
        let mut s = Store::new();
        s.set(1, None, "x", "saved", None).unwrap();
        s.snapshot(1, "snap").unwrap();
        s.set(2, None, "x", "changed", None).unwrap();
        assert_eq!(s.restore(2, "snap"), Ok(3));
        assert_eq!(s.get(2, None, "x"), Ok(Some("saved".into())));
    }

    #[test]
    fn public_scan_aggregate() {
        let mut s = Store::new();
        s.set(0, None, "a", "2", None).unwrap();
        s.set(0, None, "b", "-1", None).unwrap();
        assert_eq!(
            s.aggregate(0, None, "", None, None),
            Ok(Aggregate { count: 2, sum: 1 })
        );
    }
}