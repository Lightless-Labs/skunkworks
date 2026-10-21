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

type Entry = (String, Option<u64>);
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
        self.purge_visible(now);
        Ok(())
    }

    fn purge(map: &mut Data, now: u64) {
        map.retain(|_, (_, deadline)| deadline.map_or(true, |d| d > now));
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
        for (at, byte) in name.bytes().enumerate() {
            let valid = if at == 0 {
                byte.is_ascii_lowercase()
            } else {
                byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'_'
            };
            if !valid {
                return Err(Error::NameInvalid { at });
            }
        }
        Ok(())
    }

    fn matches(key: &str, start: &str, end: Option<&str>, prefix: Option<&str>) -> bool {
        key >= start
            && end.map_or(true, |e| key < e)
            && prefix.map_or(true, |p| key.starts_with(p))
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
        let map = self.scopes.pop().ok_or(Error::NoTransaction)?;
        if let Some(parent) = self.scopes.last_mut() {
            *parent = map;
        } else {
            self.committed = map;
        }
        Ok(())
    }

    pub fn rollback(&mut self, now: u64) -> Result<(), Error> {
        self.admit(now)?;
        if self.scopes.pop().is_none() {
            return Err(Error::NoTransaction);
        }
        self.purge_visible(now);
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
            Some(d) => Some(now.checked_add(d).ok_or(Error::DeadlineOverflow)?),
            None => None,
        };
        let map = self.visible_mut();
        if deadline == Some(now) {
            map.remove(key);
        } else {
            map.insert(key.to_owned(), (value.to_owned(), deadline));
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
        Ok(self.visible().get(key).map(|(value, _)| value.clone()))
    }

    pub fn count(&mut self, now: u64, value: &str) -> Result<usize, Error> {
        self.admit(now)?;
        Ok(self.visible().values().filter(|(v, _)| v == value).count())
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
        let mut map = self
            .snapshots
            .get(name)
            .cloned()
            .ok_or_else(|| Error::SnapshotMissing(name.to_owned()))?;
        Self::purge(&mut map, now);
        *self.visible_mut() = map;
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
        if end.map_or(false, |e| e <= start) {
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

        if limit == 0 {
            return Ok(Page {
                items: Vec::new(),
                next: None,
            });
        }

        let mut items = Vec::new();
        let mut has_more = false;
        let mut skipped = 0usize;
        for (key, (value, _)) in self.visible() {
            if !Self::matches(key, start, end, prefix) {
                continue;
            }
            if let Some(c) = cursor {
                if key <= &c.last {
                    continue;
                }
            } else if skipped < offset {
                skipped += 1;
                continue;
            }

            if items.len() == limit {
                has_more = true;
                break;
            }
            items.push((key.clone(), value.clone()));
        }

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

    pub fn aggregate(
        &mut self,
        now: u64,
        start: &str,
        end: Option<&str>,
        prefix: Option<&str>,
    ) -> Result<Aggregate, Error> {
        self.admit(now)?;
        if end.map_or(false, |e| e <= start) {
            return Err(Error::InvalidRange);
        }

        let mut count = 0usize;
        let mut sum = 0i64;
        for (key, (value, _)) in self.visible() {
            if !Self::matches(key, start, end, prefix) {
                continue;
            }
            let number = parse_canonical_i64(value).ok_or_else(|| Error::InvalidNumber {
                key: key.clone(),
                value: value.clone(),
            })?;
            sum = sum
                .checked_add(number)
                .ok_or_else(|| Error::SumOverflow { key: key.clone() })?;
            count += 1;
        }
        Ok(Aggregate { count, sum })
    }
}

fn parse_canonical_i64(value: &str) -> Option<i64> {
    let bytes = value.as_bytes();
    if bytes.is_empty() {
        return None;
    }

    let digits = if bytes[0] == b'-' {
        if bytes.len() < 2 {
            return None;
        }
        &bytes[1..]
    } else {
        bytes
    };

    if digits[0] == b'0' {
        if digits.len() != 1 || bytes[0] == b'-' {
            return None;
        }
    } else if !(b'1'..=b'9').contains(&digits[0]) {
        return None;
    }
    if !digits.iter().all(u8::is_ascii_digit) {
        return None;
    }
    value.parse::<i64>().ok()
}