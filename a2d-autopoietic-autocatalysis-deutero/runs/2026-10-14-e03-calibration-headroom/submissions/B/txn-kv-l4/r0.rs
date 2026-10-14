use std::collections::HashMap;

#[derive(Debug, PartialEq, Eq)]
pub enum Error {
    NoTransaction,
    UnknownSnapshot(u64),
    ClockWentBackwards { last: u64, now: u64 },
    DeadlineOverflow,
}

#[derive(Clone)]
struct Entry {
    value: String,
    deadline: Option<u64>,
}

pub struct Store {
    entries: HashMap<String, Entry>,
    transactions: Vec<HashMap<String, Entry>>,
    snapshots: HashMap<u64, HashMap<String, Entry>>,
    last_time: u64,
}

impl Store {
    pub fn new() -> Self {
        Self {
            entries: HashMap::new(),
            transactions: Vec::new(),
            snapshots: HashMap::new(),
            last_time: 0,
        }
    }

    fn advance(&mut self, now: u64) -> Result<(), Error> {
        if now < self.last_time {
            return Err(Error::ClockWentBackwards {
                last: self.last_time,
                now,
            });
        }

        self.last_time = now;
        self.entries
            .retain(|_, entry| entry.deadline.is_none_or(|deadline| deadline > now));
        Ok(())
    }

    fn live_copy(entries: &HashMap<String, Entry>, now: u64) -> HashMap<String, Entry> {
        entries
            .iter()
            .filter(|(_, entry)| entry.deadline.is_none_or(|deadline| deadline > now))
            .map(|(key, entry)| (key.clone(), entry.clone()))
            .collect()
    }

    pub fn set(
        &mut self,
        key: &str,
        value: &str,
        ttl: Option<u64>,
        now: u64,
    ) -> Result<(), Error> {
        self.advance(now)?;

        let deadline = match ttl {
            Some(ttl) => Some(now.checked_add(ttl).ok_or(Error::DeadlineOverflow)?),
            None => None,
        };

        if deadline.is_some_and(|deadline| deadline <= now) {
            self.entries.remove(key);
        } else {
            self.entries.insert(
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
        self.advance(now)?;
        Ok(self.entries.get(key).map(|entry| entry.value.clone()))
    }

    pub fn delete(&mut self, key: &str, now: u64) -> Result<bool, Error> {
        self.advance(now)?;
        Ok(self.entries.remove(key).is_some())
    }

    pub fn count(&mut self, value: &str, now: u64) -> Result<usize, Error> {
        self.advance(now)?;
        Ok(self
            .entries
            .values()
            .filter(|entry| entry.value == value)
            .count())
    }

    pub fn begin(&mut self, now: u64) -> Result<(), Error> {
        self.advance(now)?;
        self.transactions.push(self.entries.clone());
        Ok(())
    }

    pub fn commit(&mut self, now: u64) -> Result<(), Error> {
        self.advance(now)?;
        if self.transactions.is_empty() {
            return Err(Error::NoTransaction);
        }
        self.transactions.clear();
        Ok(())
    }

    pub fn rollback(&mut self, now: u64) -> Result<(), Error> {
        self.advance(now)?;
        let saved = self.transactions.pop().ok_or(Error::NoTransaction)?;
        self.entries = Self::live_copy(&saved, now);
        Ok(())
    }

    pub fn snapshot(&mut self, id: u64, now: u64) -> Result<(), Error> {
        self.advance(now)?;
        self.snapshots.insert(id, self.entries.clone());
        Ok(())
    }

    pub fn restore(&mut self, id: u64, now: u64) -> Result<(), Error> {
        self.advance(now)?;
        let saved = self
            .snapshots
            .get(&id)
            .ok_or(Error::UnknownSnapshot(id))?;
        self.entries = Self::live_copy(saved, now);
        Ok(())
    }
}