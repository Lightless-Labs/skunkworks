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
    pub fn new() -> Self {
        Self {
            committed: Data::new(),
            scopes: Vec::new(),
            snapshots: BTreeMap::new(),
            last_time: 0,
        }
    }

    fn current(&self) -> &Data {
        self.scopes.last().unwrap_or(&self.committed)
    }

    fn current_mut(&mut self) -> &mut Data {
        match self.scopes.len() {
            0 => &mut self.committed,
            n => &mut self.scopes[n - 1],
        }
    }

    fn tick(&mut self, now: u64) -> Result<(), Error> {
        if now < self.last_time {
            return Err(Error::TimeBackwards {
                previous: self.last_time,
                now,
            });
        }
        self.last_time = now;
        let data = self.current_mut();
        data.retain(|_, e| match e.deadline {
            Some(deadline) => deadline > now,
            None => true,
        });
        Ok(())
    }

    fn key_check(key: &str) -> Result<(), Error> {
        if key.is_empty() {
            Err(Error::KeyEmpty)
        } else {
            Ok(())
        }
    }

    fn name_check(name: &str) -> Result<(), Error> {
        if name.is_empty() {
            return Err(Error::NameEmpty);
        }
        for (offset, ch) in name.char_indices() {
            let valid = if offset == 0 {
                ch.is_ascii_lowercase()
            } else {
                ch.is_ascii_lowercase() || ch.is_ascii_digit() || ch == '_'
            };
            if !valid {
                return Err(Error::NameInvalid { at: offset });
            }
        }
        Ok(())
    }

    pub fn begin(&mut self, now: u64) -> Result<(), Error> {
        self.tick(now)?;
        if self.scopes.len() == 4 {
            return Err(Error::DepthLimit);
        }
        let copy = self.current().clone();
        self.scopes.push(copy);
        Ok(())
    }

    pub fn commit(&mut self, now: u64) -> Result<(), Error> {
        self.tick(now)?;
        let result = match self.scopes.pop() {
            Some(data) => data,
            None => return Err(Error::NoTransaction),
        };
        *self.current_mut() = result;
        Ok(())
    }

    pub fn rollback(&mut self, now: u64) -> Result<(), Error> {
        self.tick(now)?;
        if self.scopes.pop().is_none() {
            return Err(Error::NoTransaction);
        }
        self.current_mut()
            .retain(|_, e| e.deadline.map_or(true, |d| d > now));
        Ok(())
    }

    pub fn set(
        &mut self,
        now: u64,
        key: &str,
        value: &str,
        ttl: Option<u64>,
    ) -> Result<(), Error> {
        self.tick(now)?;
        Self::key_check(key)?;
        let deadline = match ttl {
            Some(d) => {
                let deadline = now.checked_add(d).ok_or(Error::DeadlineOverflow)?;
                if deadline == now {
                    self.current_mut().remove(key);
                    return Ok(());
                }
                Some(deadline)
            }
            None => None,
        };
        self.current_mut().insert(
            key.to_owned(),
            Entry {
                value: value.to_owned(),
                deadline,
            },
        );
        Ok(())
    }

    pub fn delete(&mut self, now: u64, key: &str) -> Result<bool, Error> {
        self.tick(now)?;
        Self::key_check(key)?;
        Ok(self.current_mut().remove(key).is_some())
    }

    pub fn get(&mut self, now: u64, key: &str) -> Result<Option<String>, Error> {
        self.tick(now)?;
        Self::key_check(key)?;
        Ok(self.current().get(key).map(|e| e.value.clone()))
    }

    pub fn count(&mut self, now: u64, value: &str) -> Result<usize, Error> {
        self.tick(now)?;
        Ok(self.current().values().filter(|e| e.value == value).count())
    }

    pub fn depth(&mut self, now: u64) -> Result<usize, Error> {
        self.tick(now)?;
        Ok(self.scopes.len())
    }

    pub fn snapshot(&mut self, now: u64, name: &str) -> Result<(), Error> {
        self.tick(now)?;
        Self::name_check(name)?;
        if !self.snapshots.contains_key(name) && self.snapshots.len() == 3 {
            return Err(Error::SnapshotLimit);
        }
        let copy = self.current().clone();
        self.snapshots.insert(name.to_owned(), copy);
        Ok(())
    }

    pub fn restore(&mut self, now: u64, name: &str) -> Result<(), Error> {
        self.tick(now)?;
        Self::name_check(name)?;
        let copy = match self.snapshots.get(name) {
            Some(data) => data.clone(),
            None => return Err(Error::SnapshotMissing(name.to_owned())),
        };
        *self.current_mut() = copy;
        self.current_mut()
            .retain(|_, e| e.deadline.map_or(true, |d| d > now));
        Ok(())
    }

    pub fn forget(&mut self, now: u64, name: &str) -> Result<(), Error> {
        self.tick(now)?;
        Self::name_check(name)?;
        if self.snapshots.remove(name).is_some() {
            Ok(())
        } else {
            Err(Error::SnapshotMissing(name.to_owned()))
        }
    }
}
