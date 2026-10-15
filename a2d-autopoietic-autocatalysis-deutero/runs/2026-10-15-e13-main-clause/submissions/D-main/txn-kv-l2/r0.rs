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

#[derive(Clone, PartialEq, Eq)]
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
        Self::purge(self.visible_mut(), now);
        Ok(())
    }

    fn visible(&self) -> &Data {
        self.scopes.last().unwrap_or(&self.committed)
    }

    fn visible_mut(&mut self) -> &mut Data {
        self.scopes.last_mut().unwrap_or(&mut self.committed)
    }

    fn purge(data: &mut Data, now: u64) {
        data.retain(|_, entry| entry.deadline.is_none_or(|deadline| deadline > now));
    }

    fn validate_key(key: &str) -> Result<(), Error> {
        if key.is_empty() {
            Err(Error::KeyEmpty)
        } else {
            Ok(())
        }
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

    pub fn begin(&mut self, now: u64) -> Result<(), Error> {
        self.admit(now)?;
        if self.scopes.len() >= 4 {
            return Err(Error::DepthLimit);
        }
        self.scopes.push(self.visible().clone());
        Ok(())
    }

    pub fn commit(&mut self, now: u64) -> Result<(), Error> {
        self.admit(now)?;
        let data = self.scopes.pop().ok_or(Error::NoTransaction)?;
        if let Some(parent) = self.scopes.last_mut() {
            *parent = data;
        } else {
            self.committed = data;
        }
        Ok(())
    }

    pub fn rollback(&mut self, now: u64) -> Result<(), Error> {
        self.admit(now)?;
        self.scopes.pop().ok_or(Error::NoTransaction)?;
        Self::purge(self.visible_mut(), now);
        Ok(())
    }

    pub fn set(
        &mut self,
        now: u64,
        key: &str,
        value: &str,
        ttl: Option<u64>,
    ) -> Result<(), Error> {
        self.admit(now)?;
        Self::validate_key(key)?;
        let deadline = match ttl {
            None => None,
            Some(d) => Some(now.checked_add(d).ok_or(Error::DeadlineOverflow)?),
        };
        let data = self.visible_mut();
        if deadline.is_some_and(|d| d <= now) {
            data.remove(key);
        } else {
            data.insert(
                key.to_owned(),
                Entry {
                    value: value.to_owned(),
                    deadline,
                },
            );
        }
        Ok(())
    }

    pub fn delete(&mut self, now: u64, key: &str) -> Result<bool, Error> {
        self.admit(now)?;
        Self::validate_key(key)?;
        Ok(self.visible_mut().remove(key).is_some())
    }

    pub fn get(&mut self, now: u64, key: &str) -> Result<Option<String>, Error> {
        self.admit(now)?;
        Self::validate_key(key)?;
        Ok(self.visible().get(key).map(|entry| entry.value.clone()))
    }

    pub fn count(&mut self, now: u64, value: &str) -> Result<usize, Error> {
        self.admit(now)?;
        Ok(self
            .visible()
            .values()
            .filter(|entry| entry.value == value)
            .count())
    }

    pub fn depth(&mut self, now: u64) -> Result<usize, Error> {
        self.admit(now)?;
        Ok(self.scopes.len())
    }

    pub fn snapshot(&mut self, now: u64, name: &str) -> Result<(), Error> {
        self.admit(now)?;
        Self::validate_name(name)?;
        if !self.snapshots.contains_key(name) && self.snapshots.len() >= 3 {
            return Err(Error::SnapshotLimit);
        }
        self.snapshots.insert(name.to_owned(), self.visible().clone());
        Ok(())
    }

    pub fn restore(&mut self, now: u64, name: &str) -> Result<(), Error> {
        self.admit(now)?;
        Self::validate_name(name)?;
        let mut data = self
            .snapshots
            .get(name)
            .cloned()
            .ok_or_else(|| Error::SnapshotMissing(name.to_owned()))?;
        Self::purge(&mut data, now);
        *self.visible_mut() = data;
        Ok(())
    }

    pub fn forget(&mut self, now: u64, name: &str) -> Result<(), Error> {
        self.admit(now)?;
        Self::validate_name(name)?;
        self.snapshots
            .remove(name)
            .map(|_| ())
            .ok_or_else(|| Error::SnapshotMissing(name.to_owned()))
    }
}

fn main() {}