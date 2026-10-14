use std::collections::BTreeMap;

#[derive(Clone)]
struct Entry {
    value: String,
    deadline: Option<u64>,
}

#[derive(Debug, PartialEq, Eq)]
pub enum Error {
    NoTransaction,
    UnknownSnapshot(u64),
    ClockWentBackwards { last: u64, now: u64 },
    DeadlineOverflow,
}

pub struct Store {
    data: BTreeMap<String, Entry>,
    frames: Vec<BTreeMap<String, Entry>>,
    snapshots: BTreeMap<u64, BTreeMap<String, Entry>>,
    last: u64,
}

impl Store {
    pub fn new() -> Self {
        Self {
            data: BTreeMap::new(),
            frames: Vec::new(),
            snapshots: BTreeMap::new(),
            last: 0,
        }
    }
    fn purge(data: &mut BTreeMap<String, Entry>, now: u64) {
        data.retain(|_, entry| entry.deadline.is_none_or(|deadline| deadline > now));
    }
    fn prepare(&mut self, now: u64) -> Result<(), Error> {
        if now < self.last {
            return Err(Error::ClockWentBackwards {
                last: self.last,
                now,
            });
        }
        self.last = now;
        Self::purge(&mut self.data, now);
        Ok(())
    }
    pub fn set(&mut self, key: &str, value: &str, ttl: Option<u64>, now: u64) -> Result<(), Error> {
        self.prepare(now)?;
        let deadline = match ttl {
            None => None,
            Some(ttl) => Some(now.checked_add(ttl).ok_or(Error::DeadlineOverflow)?),
        };
        if deadline == Some(now) {
            self.data.remove(key);
        } else {
            self.data.insert(
                key.to_owned(),
                Entry {
                    value: value.to_owned(),
                    deadline,
                },
            );
        }
        Ok(())
    }
    pub fn get(&mut self, key: &str, now: u64) -> Result<Option<String>, Error> {
        self.prepare(now)?;
        Ok(self.data.get(key).map(|entry| entry.value.clone()))
    }
    pub fn delete(&mut self, key: &str, now: u64) -> Result<bool, Error> {
        self.prepare(now)?;
        Ok(self.data.remove(key).is_some())
    }
    pub fn count(&mut self, value: &str, now: u64) -> Result<usize, Error> {
        self.prepare(now)?;
        Ok(self
            .data
            .values()
            .filter(|entry| entry.value == value)
            .count())
    }
    pub fn begin(&mut self, now: u64) -> Result<(), Error> {
        self.prepare(now)?;
        self.frames.push(self.data.clone());
        Ok(())
    }
    pub fn commit(&mut self, now: u64) -> Result<(), Error> {
        self.prepare(now)?;
        if self.frames.is_empty() {
            return Err(Error::NoTransaction);
        }
        self.frames.clear();
        Ok(())
    }
    pub fn rollback(&mut self, now: u64) -> Result<(), Error> {
        self.prepare(now)?;
        let mut saved = self.frames.pop().ok_or(Error::NoTransaction)?;
        Self::purge(&mut saved, now);
        self.data = saved;
        Ok(())
    }
    pub fn snapshot(&mut self, id: u64, now: u64) -> Result<(), Error> {
        self.prepare(now)?;
        self.snapshots.insert(id, self.data.clone());
        Ok(())
    }
    pub fn restore(&mut self, id: u64, now: u64) -> Result<(), Error> {
        self.prepare(now)?;
        let mut saved = self
            .snapshots
            .get(&id)
            .ok_or(Error::UnknownSnapshot(id))?
            .clone();
        Self::purge(&mut saved, now);
        self.data = saved;
        Ok(())
    }
}
