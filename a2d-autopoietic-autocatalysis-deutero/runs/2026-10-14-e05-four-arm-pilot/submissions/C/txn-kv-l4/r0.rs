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

#[derive(Debug, Clone, PartialEq, Eq)]
struct Entry {
    value: String,
    deadline: Option<u64>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct Version {
    revision: u64,
    entry: Option<Entry>,
}

#[derive(Debug, Clone)]
struct Transaction {
    begin_revision: u64,
    read_only: bool,
    writes: BTreeMap<String, Option<Entry>>,
}

pub struct Store {
    clock: u64,
    revision: u64,
    data: BTreeMap<String, Vec<Version>>,
    transactions: BTreeMap<u64, Transaction>,
    snapshots: BTreeMap<String, BTreeMap<String, Entry>>,
}

impl Default for Store {
    fn default() -> Self {
        Self::new()
    }
}

impl Store {
    /// Creates an empty store with a zero logical clock and revision.
    pub fn new() -> Self {
        Self {
            clock: 0,
            revision: 0,
            data: BTreeMap::new(),
            transactions: BTreeMap::new(),
            snapshots: BTreeMap::new(),
        }
    }

    fn admit_time(&mut self, now: u64) -> Result<(), Error> {
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

    fn valid_key(key: &str) -> Result<(), Error> {
        if key.is_empty() {
            Err(Error::KeyEmpty)
        } else {
            Ok(())
        }
    }

    fn selected_entry(&self, key: &str, tx: Option<u64>) -> Option<Entry> {
        if let Some(id) = tx {
            let transaction = self.transactions.get(&id)?;
            if let Some(write) = transaction.writes.get(key) {
                return write.clone();
            }
        }

        let versions = self.data.get(key)?;
        let max_revision = tx
            .and_then(|id| self.transactions.get(&id).map(|t| t.begin_revision));
        versions
            .iter()
            .rev()
            .find(|v| max_revision.is_none_or(|r| v.revision <= r))
            .and_then(|v| v.entry.clone())
    }

    fn live(entry: &Entry, now: u64) -> bool {
        entry.deadline.is_none_or(|deadline| deadline > now)
    }

    fn visible(&self, key: &str, tx: Option<u64>, now: u64) -> Option<Entry> {
        self.selected_entry(key, tx)
            .filter(|entry| Self::live(entry, now))
    }

    fn visible_map(&self, tx: Option<u64>, now: u64) -> BTreeMap<String, Entry> {
        let mut keys: BTreeSet<String> = self.data.keys().cloned().collect();
        if let Some(id) = tx {
            if let Some(transaction) = self.transactions.get(&id) {
                keys.extend(transaction.writes.keys().cloned());
            }
        }

        keys.into_iter()
            .filter_map(|key| self.visible(&key, tx, now).map(|entry| (key, entry)))
            .collect()
    }

    fn append_committed(&mut self, key: String, entry: Option<Entry>) {
        self.revision += 1;
        self.data.entry(key).or_default().push(Version {
            revision: self.revision,
            entry,
        });
    }

    fn validate_name(name: &str) -> Result<(), Error> {
        if name.is_empty() {
            return Err(Error::NameEmpty);
        }
        for (i, byte) in name.bytes().enumerate() {
            let valid = if i == 0 {
                byte.is_ascii_lowercase()
            } else {
                byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'_'
            };
            if !valid {
                return Err(Error::NameInvalid { at: i });
            }
        }
        Ok(())
    }

    fn range_matches(key: &str, start: &str, end: Option<&str>, prefix: Option<&str>) -> bool {
        key >= start
            && end.is_none_or(|bound| key < bound)
            && prefix.is_none_or(|p| key.starts_with(p))
    }

    /// Begins a transaction at the current committed revision.
    pub fn begin(&mut self, now: u64, id: u64, read_only: bool) -> Result<(), Error> {
        self.admit_time(now)?;
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

    /// Commits a transaction, returning the resulting committed revision.
    pub fn commit(&mut self, now: u64, id: u64) -> Result<u64, Error> {
        self.admit_time(now)?;
        let transaction = self
            .transactions
            .get(&id)
            .cloned()
            .ok_or(Error::UnknownTransaction(id))?;

        if transaction.writes.is_empty() {
            self.transactions.remove(&id);
            return Ok(self.revision);
        }

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

        self.revision += 1;
        let revision = self.revision;
        for (key, entry) in transaction.writes {
            self.data.entry(key).or_default().push(Version {
                revision,
                entry,
            });
        }
        self.transactions.remove(&id);
        Ok(revision)
    }

    /// Rolls back a transaction and discards its staged writes.
    pub fn rollback(&mut self, now: u64, id: u64) -> Result<(), Error> {
        self.admit_time(now)?;
        if self.transactions.remove(&id).is_none() {
            return Err(Error::UnknownTransaction(id));
        }
        Ok(())
    }

    /// Sets a key either in a transaction or directly in committed data.
    pub fn set(
        &mut self,
        now: u64,
        tx: Option<u64>,
        key: &str,
        value: &str,
        ttl: Option<u64>,
    ) -> Result<(), Error> {
        self.admit_time(now)?;
        self.validate_tx(tx)?;
        Self::valid_key(key)?;

        if let Some(id) = tx {
            if self.transactions.get(&id).is_some_and(|t| t.read_only) {
                return Err(Error::ReadOnly(id));
            }
        }

        let deadline = match ttl {
            Some(duration) => Some(
                now.checked_add(duration)
                    .ok_or(Error::DeadlineOverflow)?,
            ),
            None => None,
        };

        let entry = if deadline == Some(now) {
            None
        } else {
            Some(Entry {
                value: value.to_string(),
                deadline,
            })
        };

        if let Some(id) = tx {
            self.transactions
                .get_mut(&id)
                .expect("validated transaction")
                .writes
                .insert(key.to_string(), entry);
        } else {
            self.append_committed(key.to_string(), entry);
        }
        Ok(())
    }

    /// Deletes a key and reports whether it was live before deletion.
    pub fn delete(&mut self, now: u64, tx: Option<u64>, key: &str) -> Result<bool, Error> {
        self.admit_time(now)?;
        self.validate_tx(tx)?;
        Self::valid_key(key)?;

        if let Some(id) = tx {
            if self.transactions.get(&id).is_some_and(|t| t.read_only) {
                return Err(Error::ReadOnly(id));
            }
        }

        let was_live = self.visible(key, tx, now).is_some();
        if let Some(id) = tx {
            self.transactions
                .get_mut(&id)
                .expect("validated transaction")
                .writes
                .insert(key.to_string(), None);
        } else {
            self.append_committed(key.to_string(), None);
        }
        Ok(was_live)
    }

    /// Reads a live key from the selected committed or transactional view.
    pub fn get(&mut self, now: u64, tx: Option<u64>, key: &str) -> Result<Option<String>, Error> {
        self.admit_time(now)?;
        self.validate_tx(tx)?;
        Self::valid_key(key)?;
        Ok(self
            .visible(key, tx, now)
            .map(|entry| entry.value))
    }

    /// Counts live keys whose value exactly matches the supplied string.
    pub fn count(&mut self, now: u64, tx: Option<u64>, value: &str) -> Result<usize, Error> {
        self.admit_time(now)?;
        self.validate_tx(tx)?;
        Ok(self
            .visible_map(tx, now)
            .values()
            .filter(|entry| entry.value == value)
            .count())
    }

    /// Returns a filtered, ordered page of visible key/value pairs.
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
        self.admit_time(now)?;
        self.validate_tx(tx)?;

        if end.is_some_and(|bound| bound <= start) {
            return Err(Error::InvalidRange);
        }

        if let Some(c) = cursor {
            if c.start != start
                || c.end.as_deref() != end
                || c.prefix.as_deref() != prefix
                || c.offset != offset
                || c.last.is_empty()
                || !Self::range_matches(&c.last, start, end, prefix)
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

        let visible = self.visible_map(tx, now);
        let mut matching: Vec<(String, String)> = visible
            .into_iter()
            .filter(|(key, _)| Self::range_matches(key, start, end, prefix))
            .filter(|(key, _)| cursor.is_none_or(|c| key.as_str() > c.last.as_str()))
            .map(|(key, entry)| (key, entry.value))
            .collect();

        if cursor.is_none() {
            matching = matching.into_iter().skip(offset).collect();
        }

        let take = matching.len().min(limit);
        let has_more = matching.len() > take;
        let items: Vec<_> = matching.into_iter().take(take).collect();

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

    /// Counts matching live entries and sums their canonical signed integers.
    pub fn aggregate(
        &mut self,
        now: u64,
        tx: Option<u64>,
        start: &str,
        end: Option<&str>,
        prefix: Option<&str>,
    ) -> Result<Aggregate, Error> {
        self.admit_time(now)?;
        self.validate_tx(tx)?;

        if end.is_some_and(|bound| bound <= start) {
            return Err(Error::InvalidRange);
        }

        let visible = self.visible_map(tx, now);
        let mut count = 0usize;
        let mut sum = 0i64;

        for (key, entry) in visible {
            if !Self::range_matches(&key, start, end, prefix) {
                continue;
            }
            let value = entry.value.as_str();
            let digits = if let Some(rest) = value.strip_prefix('-') {
                if rest.is_empty()
                    || rest == "0"
                    || !rest.as_bytes()[0].is_ascii_digit()
                    || rest.as_bytes()[0] == b'0'
                    || !rest.bytes().all(|b| b.is_ascii_digit())
                {
                    return Err(Error::InvalidNumber {
                        key,
                        value: value.to_string(),
                    });
                }
                value
            } else {
                if value.is_empty()
                    || !value.as_bytes()[0].is_ascii_digit()
                    || (value.len() > 1 && value.as_bytes()[0] == b'0')
                    || !value.bytes().all(|b| b.is_ascii_digit())
                {
                    return Err(Error::InvalidNumber {
                        key,
                        value: value.to_string(),
                    });
                }
                value
            };

            let parsed = digits.parse::<i64>().map_err(|_| Error::InvalidNumber {
                key: key.clone(),
                value: value.to_string(),
            })?;
            sum = sum.checked_add(parsed).ok_or_else(|| Error::SumOverflow {
                key: key.clone(),
            })?;
            count += 1;
        }

        Ok(Aggregate { count, sum })
    }

    /// Saves a materialized copy of currently live committed data.
    pub fn snapshot(&mut self, now: u64, name: &str) -> Result<(), Error> {
        self.admit_time(now)?;
        Self::validate_name(name)?;

        if !self.snapshots.contains_key(name) && self.snapshots.len() >= 3 {
            return Err(Error::SnapshotLimit);
        }

        let saved = self
            .visible_map(None, now)
            .into_iter()
            .collect::<BTreeMap<_, _>>();
        self.snapshots.insert(name.to_string(), saved);
        Ok(())
    }

    /// Restores a named snapshot and returns the resulting revision.
    pub fn restore(&mut self, now: u64, name: &str) -> Result<u64, Error> {
        self.admit_time(now)?;
        Self::validate_name(name)?;
        let saved = self
            .snapshots
            .get(name)
            .cloned()
            .ok_or_else(|| Error::SnapshotMissing(name.to_string()))?;

        if !self.transactions.is_empty() {
            return Err(Error::ActiveTransactions);
        }

        let target: BTreeMap<String, Entry> = saved
            .into_iter()
            .filter(|(_, entry)| Self::live(entry, now))
            .collect();
        let current = self.visible_map(None, now);

        let mut keys: BTreeSet<String> = current.keys().cloned().collect();
        keys.extend(target.keys().cloned());

        if keys.is_empty() {
            return Ok(self.revision);
        }

        self.revision += 1;
        let revision = self.revision;
        for key in keys {
            let entry = target.get(&key).cloned();
            self.data.entry(key).or_default().push(Version {
                revision,
                entry,
            });
        }
        Ok(revision)
    }

    /// Forgets a named snapshot.
    pub fn forget(&mut self, now: u64, name: &str) -> Result<(), Error> {
        self.admit_time(now)?;
        Self::validate_name(name)?;
        if self.snapshots.remove(name).is_none() {
            return Err(Error::SnapshotMissing(name.to_string()));
        }
        Ok(())
    }

    /// Removes committed versions that are not needed by current transactions.
    pub fn gc(&mut self, now: u64) -> Result<usize, Error> {
        self.admit_time(now)?;
        let mut removed = 0usize;

        for versions in self.data.values_mut() {
            let mut keep = BTreeSet::new();
            if let Some(last) = versions.len().checked_sub(1) {
                keep.insert(last);
            }

            for transaction in self.transactions.values() {
                if let Some(index) = versions
                    .iter()
                    .enumerate()
                    .rev()
                    .find(|(_, version)| version.revision <= transaction.begin_revision)
                    .map(|(index, _)| index)
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

    /// Returns counts for retained versions, live data, transactions, and snapshots.
    pub fn stats(&mut self, now: u64) -> Result<Stats, Error> {
        self.admit_time(now)?;
        let versions = self.data.values().map(Vec::len).sum();
        let tombstones = self
            .data
            .values()
            .flat_map(|versions| versions.iter())
            .filter(|version| version.entry.is_none())
            .count();
        let live_keys = self.visible_map(None, now).len();

        Ok(Stats {
            clock: self.clock,
            revision: self.revision,
            keys: self.data.values().filter(|versions| !versions.is_empty()).count(),
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

fn main() {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normal_transaction_isolation_and_commit() {
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
    fn invalid_inputs_and_time_ordering() {
        let mut store = Store::new();
        assert_eq!(store.get(1, Some(99), ""), Err(Error::UnknownTransaction(99)));
        assert_eq!(store.get(1, None, ""), Err(Error::KeyEmpty));
        assert_eq!(
            store.set(u64::MAX, None, "x", "v", Some(1)),
            Err(Error::DeadlineOverflow)
        );
        assert_eq!(
            store.stats(u64::MAX - 1),
            Err(Error::TimeBackwards {
                previous: u64::MAX,
                now: u64::MAX - 1
            })
        );
    }

    #[test]
    fn scan_aggregate_and_cursor_resume() {
        let mut store = Store::new();
        for (key, value) in [("a", "2"), ("b", "-1"), ("c", "4")] {
            store.set(0, None, key, value, None).unwrap();
        }
        let page = store.scan(0, None, "", None, None, 0, 2, None).unwrap();
        assert_eq!(page.items.len(), 2);
        assert_eq!(
            store
                .scan(0, None, "", None, None, 0, 2, page.next.as_ref())
                .unwrap()
                .items,
            vec![("c".into(), "4".into())]
        );
        assert_eq!(
            store.aggregate(0, None, "", None, None),
            Ok(Aggregate { count: 3, sum: 5 })
        );
    }

    #[test]
    fn snapshot_restore_gc_end_to_end() {
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