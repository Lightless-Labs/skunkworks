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
    last_time: u64,
}

impl Store {
    /// Creates an empty store with its logical clock set to zero.
    pub fn new() -> Self {
        Self {
            committed: BTreeMap::new(),
            scopes: Vec::new(),
            snapshots: BTreeMap::new(),
            last_time: 0,
        }
    }

    fn admit(&mut self, now: u64) -> Result<(), Error> {
        if now < self.last_time {
            return Err(Error::TimeBackwards {
                previous: self.last_time,
                now,
            });
        }
        self.last_time = now;
        self.purge_visible(now);
        Ok(())
    }

    fn purge(map: &mut Data, now: u64) {
        map.retain(|_, entry| entry.deadline.map_or(true, |deadline| deadline > now));
    }

    fn purge_visible(&mut self, now: u64) {
        if let Some(map) = self.scopes.last_mut() {
            Self::purge(map, now);
        } else {
            Self::purge(&mut self.committed, now);
        }
    }

    fn visible(&self) -> &Data {
        self.scopes.last().unwrap_or(&self.committed)
    }

    fn visible_mut(&mut self) -> &mut Data {
        self.scopes.last_mut().unwrap_or(&mut self.committed)
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

    /// Opens a working scope copied from the currently visible map.
    pub fn begin(&mut self, now: u64) -> Result<(), Error> {
        self.admit(now)?;
        if self.scopes.len() >= 4 {
            return Err(Error::DepthLimit);
        }
        self.scopes.push(self.visible().clone());
        Ok(())
    }

    /// Commits the innermost scope into its parent or the committed map.
    pub fn commit(&mut self, now: u64) -> Result<(), Error> {
        self.admit(now)?;
        let map = self.scopes.pop().ok_or(Error::NoTransaction)?;
        if let Some(parent) = self.scopes.last_mut() {
            *parent = map;
        } else {
            self.committed = map;
        }
        Ok(())
    }

    /// Discards the innermost scope and reveals its parent map.
    pub fn rollback(&mut self, now: u64) -> Result<(), Error> {
        self.admit(now)?;
        self.scopes.pop().ok_or(Error::NoTransaction)?;
        self.purge_visible(now);
        Ok(())
    }

    /// Inserts or replaces a key, optionally setting an absolute expiry deadline.
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
            key.to_string(),
            Entry {
                value: value.to_string(),
                deadline,
            },
        );
        Ok(())
    }

    /// Removes a key and reports whether it was present in the visible map.
    pub fn delete(&mut self, now: u64, key: &str) -> Result<bool, Error> {
        self.admit(now)?;
        if key.is_empty() {
            return Err(Error::KeyEmpty);
        }
        Ok(self.visible_mut().remove(key).is_some())
    }

    /// Returns an owned copy of a live value, or `None` when the key is absent.
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

    /// Saves a copy of the visible map under a validated name.
    pub fn snapshot(&mut self, now: u64, name: &str) -> Result<(), Error> {
        self.admit(now)?;
        Self::valid_name(name)?;
        if !self.snapshots.contains_key(name) && self.snapshots.len() >= 3 {
            return Err(Error::SnapshotLimit);
        }
        self.snapshots
            .insert(name.to_string(), self.visible().clone());
        Ok(())
    }

    /// Replaces the visible map with a live, purged copy of a named snapshot.
    pub fn restore(&mut self, now: u64, name: &str) -> Result<(), Error> {
        self.admit(now)?;
        Self::valid_name(name)?;
        let mut map = self
            .snapshots
            .get(name)
            .cloned()
            .ok_or_else(|| Error::SnapshotMissing(name.to_string()))?;
        Self::purge(&mut map, now);
        *self.visible_mut() = map;
        Ok(())
    }

    /// Removes a named snapshot without changing data or scopes.
    pub fn forget(&mut self, now: u64, name: &str) -> Result<(), Error> {
        self.admit(now)?;
        Self::valid_name(name)?;
        self.snapshots
            .remove(name)
            .map(|_| ())
            .ok_or_else(|| Error::SnapshotMissing(name.to_string()))
    }
}

fn main() {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ttl_and_expiration() {
        let mut store = Store::new();
        store.set(2, "red", "9", Some(5)).unwrap();
        assert_eq!(store.get(3, "red"), Ok(Some("9".into())));
        assert_eq!(store.count(3, "9"), Ok(1));
        assert_eq!(store.get(7, "red"), Ok(None));
    }

    #[test]
    fn invalid_inputs_and_clock() {
        let mut store = Store::new();
        assert_eq!(store.get(1, ""), Err(Error::KeyEmpty));
        assert_eq!(store.snapshot(1, "Upper"), Err(Error::NameInvalid { at: 0 }));
        assert_eq!(
            store.depth(0),
            Err(Error::TimeBackwards {
                previous: 1,
                now: 0
            })
        );
    }

    #[test]
    fn scopes_and_snapshots_end_to_end() {
        let mut store = Store::new();
        store.set(1, "key", "base", None).unwrap();
        store.snapshot(1, "saved").unwrap();
        store.begin(2).unwrap();
        store.set(2, "key", "working", None).unwrap();
        store.restore(2, "saved").unwrap();
        assert_eq!(store.get(2, "key"), Ok(Some("base".into())));
        store.rollback(3).unwrap();
        assert_eq!(store.get(3, "key"), Ok(Some("base".into())));
        store.forget(3, "saved").unwrap();
    }

    #[test]
    fn public_scopes_and_time_error() {
        let mut store = Store::new();
        store.begin(1).unwrap();
        store.set(1, "key", "v", None).unwrap();
        store.begin(2).unwrap();
        store.set(2, "key", "w", None).unwrap();
        store.commit(2).unwrap();
        assert_eq!(store.depth(2), Ok(1));
        store.rollback(3).unwrap();
        assert_eq!(store.get(3, "key"), Ok(None));
        assert_eq!(
            store.get(2, "key"),
            Err(Error::TimeBackwards {
                previous: 3,
                now: 2
            })
        );
    }
}