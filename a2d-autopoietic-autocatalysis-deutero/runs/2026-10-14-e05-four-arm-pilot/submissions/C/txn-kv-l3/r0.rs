use std::collections::BTreeMap;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Error {
    KeyEmpty,
    NameEmpty,
    NameInvalid { at: usize },
    SnapshotLimit,
    NoTransaction,
    DepthLimit,
    SnapshotMissing(String),
    TimeBackwards { previous: u64, now: u64 },
    DeadlineOverflow,
    InvalidRange,
    CursorMismatch,
    InvalidNumber { key: String, value: String },
    SumOverflow { key: String },
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

type Data = BTreeMap<String, Entry>;

pub struct Store {
    committed: Data,
    scopes: Vec<Data>,
    snapshots: BTreeMap<String, Data>,
    clock: u64,
}

impl Store {
    /// Creates an empty store with its logical clock set to zero.
    pub fn new() -> Self {
        Self {
            committed: BTreeMap::new(),
            scopes: Vec::new(),
            snapshots: BTreeMap::new(),
            clock: 0,
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
        self.purge_visible(now);
        Ok(())
    }

    fn visible(&self) -> &Data {
        self.scopes.last().unwrap_or(&self.committed)
    }

    fn visible_mut(&mut self) -> &mut Data {
        self.scopes.last_mut().unwrap_or(&mut self.committed)
    }

    fn purge_data(data: &mut Data, now: u64) {
        data.retain(|_, entry| entry.deadline.is_none_or(|deadline| deadline > now));
    }

    fn purge_visible(&mut self, now: u64) {
        Self::purge_data(self.visible_mut(), now);
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

    fn matches(key: &str, start: &str, end: Option<&str>, prefix: Option<&str>) -> bool {
        key >= start
            && end.is_none_or(|bound| key < bound)
            && prefix.is_none_or(|p| key.starts_with(p))
    }

    /// Begins a working scope copied from the current live map.
    pub fn begin(&mut self, now: u64) -> Result<(), Error> {
        self.admit(now)?;
        if self.scopes.len() >= 4 {
            return Err(Error::DepthLimit);
        }
        self.scopes.push(self.visible().clone());
        Ok(())
    }

    /// Commits the innermost working scope into its parent or committed map.
    pub fn commit(&mut self, now: u64) -> Result<(), Error> {
        self.admit(now)?;
        let Some(data) = self.scopes.pop() else {
            return Err(Error::NoTransaction);
        };
        if let Some(parent) = self.scopes.last_mut() {
            *parent = data;
        } else {
            self.committed = data;
        }
        Ok(())
    }

    /// Discards the innermost working scope.
    pub fn rollback(&mut self, now: u64) -> Result<(), Error> {
        self.admit(now)?;
        if self.scopes.pop().is_none() {
            return Err(Error::NoTransaction);
        }
        self.purge_visible(now);
        Ok(())
    }

    /// Sets a key to a value, optionally expiring it after the given duration.
    pub fn set(
        &mut self,
        now: u64,
        key: &str,
        value: &str,
        ttl: Option<u64>,
    ) -> Result<(), Error> {
        self.admit(now)?;
        if key.is_empty() {
            return Err(Error::KeyEmpty);
        }
        let deadline = match ttl {
            None => None,
            Some(0) => {
                self.visible_mut().remove(key);
                return Ok(());
            }
            Some(duration) => Some(now.checked_add(duration).ok_or(Error::DeadlineOverflow)?),
        };
        self.visible_mut().insert(
            key.to_owned(),
            Entry {
                value: value.to_owned(),
                deadline,
            },
        );
        Ok(())
    }

    /// Deletes a key and reports whether it was present.
    pub fn delete(&mut self, now: u64, key: &str) -> Result<bool, Error> {
        self.admit(now)?;
        if key.is_empty() {
            return Err(Error::KeyEmpty);
        }
        Ok(self.visible_mut().remove(key).is_some())
    }

    /// Gets an owned copy of a live value, if present.
    pub fn get(&mut self, now: u64, key: &str) -> Result<Option<String>, Error> {
        self.admit(now)?;
        if key.is_empty() {
            return Err(Error::KeyEmpty);
        }
        Ok(self.visible().get(key).map(|entry| entry.value.clone()))
    }

    /// Counts visible keys whose complete value equals the supplied value.
    pub fn count(&mut self, now: u64, value: &str) -> Result<usize, Error> {
        self.admit(now)?;
        Ok(self
            .visible()
            .values()
            .filter(|entry| entry.value == value)
            .count())
    }

    /// Returns the number of open working scopes.
    pub fn depth(&mut self, now: u64) -> Result<usize, Error> {
        self.admit(now)?;
        Ok(self.scopes.len())
    }

    /// Saves a copy of the current visible map under a validated name.
    pub fn snapshot(&mut self, now: u64, name: &str) -> Result<(), Error> {
        self.admit(now)?;
        Self::validate_name(name)?;
        if !self.snapshots.contains_key(name) && self.snapshots.len() >= 3 {
            return Err(Error::SnapshotLimit);
        }
        self.snapshots
            .insert(name.to_owned(), self.visible().clone());
        Ok(())
    }

    /// Replaces the visible map with a copy of a named snapshot.
    pub fn restore(&mut self, now: u64, name: &str) -> Result<(), Error> {
        self.admit(now)?;
        Self::validate_name(name)?;
        let mut data = self
            .snapshots
            .get(name)
            .cloned()
            .ok_or_else(|| Error::SnapshotMissing(name.to_owned()))?;
        Self::purge_data(&mut data, now);
        *self.visible_mut() = data;
        Ok(())
    }

    /// Removes a named snapshot.
    pub fn forget(&mut self, now: u64, name: &str) -> Result<(), Error> {
        self.admit(now)?;
        Self::validate_name(name)?;
        if self.snapshots.remove(name).is_none() {
            return Err(Error::SnapshotMissing(name.to_owned()));
        }
        Ok(())
    }

    /// Returns a sorted page of matching entries, optionally resuming from a cursor.
    pub fn scan(
        &mut self,
        now: u64,
        start: &str,
        end: Option<&str>,
        prefix: Option<&str>,
        offset: usize,
        limit: usize,
        cursor: Option<&Cursor>,
    ) -> Result<Page, Error> {
        self.admit(now)?;
        if end.is_some_and(|bound| bound <= start) {
            return Err(Error::InvalidRange);
        }
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

        let mut matching: Vec<(&String, &Entry)> = self
            .visible()
            .iter()
            .filter(|(key, _)| Self::matches(key, start, end, prefix))
            .filter(|(key, _)| cursor.is_none_or(|c| key.as_str() > c.last.as_str()))
            .collect();

        if cursor.is_none() {
            matching = matching.into_iter().skip(offset).collect();
        }

        let has_more = matching.len() > limit;
        let items: Vec<(String, String)> = matching
            .into_iter()
            .take(limit)
            .map(|(key, entry)| (key.clone(), entry.value.clone()))
            .collect();

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

    /// Aggregates matching entries using canonical signed decimal integers.
    pub fn aggregate(
        &mut self,
        now: u64,
        start: &str,
        end: Option<&str>,
        prefix: Option<&str>,
    ) -> Result<Aggregate, Error> {
        self.admit(now)?;
        if end.is_some_and(|bound| bound <= start) {
            return Err(Error::InvalidRange);
        }

        let mut count = 0usize;
        let mut sum = 0i64;
        for (key, entry) in self.visible() {
            if !Self::matches(key, start, end, prefix) {
                continue;
            }
            let Some(value) = parse_canonical_i64(&entry.value) else {
                return Err(Error::InvalidNumber {
                    key: key.clone(),
                    value: entry.value.clone(),
                });
            };
            sum = sum.checked_add(value).ok_or_else(|| Error::SumOverflow {
                key: key.clone(),
            })?;
            count += 1;
        }
        Ok(Aggregate { count, sum })
    }
}

fn parse_canonical_i64(text: &str) -> Option<i64> {
    if text == "0" {
        return Some(0);
    }
    let digits = text.strip_prefix('-').unwrap_or(text);
    if digits.is_empty()
        || digits.starts_with('0')
        || !digits.bytes().all(|b| b.is_ascii_digit())
    {
        return None;
    }
    if text.starts_with('-') && digits == "0" {
        return None;
    }
    text.parse::<i64>().ok()
}

fn main() {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normal_set_get_and_count() {
        let mut store = Store::new();
        store.set(0, "a", "", None).unwrap();
        store.set(0, "b", "", None).unwrap();
        assert_eq!(store.get(0, "a"), Ok(Some(String::new())));
        assert_eq!(store.count(0, ""), Ok(2));
    }

    #[test]
    fn invalid_key_and_time() {
        let mut store = Store::new();
        assert_eq!(store.get(0, ""), Err(Error::KeyEmpty));
        assert_eq!(
            store.depth(0),
            Err(Error::TimeBackwards {
                previous: 0,
                now: 0
            })
        );
    }

    #[test]
    fn scope_snapshot_and_aggregate_end_to_end() {
        let mut store = Store::new();
        store.set(1, "n", "7", None).unwrap();
        store.snapshot(1, "saved").unwrap();
        store.begin(2).unwrap();
        store.set(2, "n", "9", None).unwrap();
        store.rollback(3).unwrap();
        assert_eq!(store.aggregate(3, "", None, None), Ok(Aggregate { count: 1, sum: 7 }));
        store.delete(4, "n").unwrap();
        store.restore(4, "saved").unwrap();
        assert_eq!(store.get(4, "n"), Ok(Some("7".to_owned())));
    }
}