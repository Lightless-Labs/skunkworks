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
    clock: u64,
    committed: Data,
    scopes: Vec<Data>,
    snapshots: BTreeMap<String, Data>,
}

impl Store {
    pub fn new() -> Self {
        Self {
            clock: 0,
            committed: BTreeMap::new(),
            scopes: Vec::new(),
            snapshots: BTreeMap::new(),
        }
    }

    fn visible(&self) -> &Data {
        self.scopes.last().unwrap_or(&self.committed)
    }

    fn visible_mut(&mut self) -> &mut Data {
        self.scopes.last_mut().unwrap_or(&mut self.committed)
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

    fn purge_visible(&mut self, now: u64) {
        self.visible_mut()
            .retain(|_, (_, deadline)| deadline.is_none_or(|d| d > now));
    }

    fn validate_name(name: &str) -> Result<(), Error> {
        if name.is_empty() {
            return Err(Error::NameEmpty);
        }
        for (i, b) in name.bytes().enumerate() {
            let valid = if i == 0 {
                b.is_ascii_lowercase()
            } else {
                b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'_'
            };
            if !valid {
                return Err(Error::NameInvalid { at: i });
            }
        }
        Ok(())
    }

    fn validate_key(key: &str) -> Result<(), Error> {
        if key.is_empty() {
            Err(Error::KeyEmpty)
        } else {
            Ok(())
        }
    }

    fn valid_range(start: &str, end: Option<&str>) -> Result<(), Error> {
        if end.is_some_and(|e| e <= start) {
            Err(Error::InvalidRange)
        } else {
            Ok(())
        }
    }

    fn matches(key: &str, start: &str, end: Option<&str>, prefix: Option<&str>) -> bool {
        key >= start
            && end.is_none_or(|e| key < e)
            && prefix.is_none_or(|p| key.starts_with(p))
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
            Some(0) => {
                self.visible_mut().remove(key);
                return Ok(());
            }
            Some(d) => Some(now.checked_add(d).ok_or(Error::DeadlineOverflow)?),
            None => None,
        };
        self.visible_mut()
            .insert(key.to_owned(), (value.to_owned(), deadline));
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
        Ok(self
            .visible()
            .values()
            .filter(|(v, _)| v == value)
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
        let data = self
            .snapshots
            .get(name)
            .cloned()
            .ok_or_else(|| Error::SnapshotMissing(name.to_owned()))?;
        *self.visible_mut() = data;
        self.purge_visible(now);
        Ok(())
    }

    pub fn forget(&mut self, now: u64, name: &str) -> Result<(), Error> {
        self.admit(now)?;
        Self::validate_name(name)?;
        if self.snapshots.remove(name).is_none() {
            return Err(Error::SnapshotMissing(name.to_owned()));
        }
        Ok(())
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
        Self::valid_range(start, end)?;

        let after = if let Some(c) = cursor {
            if c.start != start
                || c.end.as_deref() != end
                || c.prefix.as_deref() != prefix
                || c.offset != offset
                || c.last.is_empty()
                || !Self::matches(&c.last, start, end, prefix)
            {
                return Err(Error::CursorMismatch);
            }
            Some(c.last.as_str())
        } else {
            None
        };

        if limit == 0 {
            return Ok(Page {
                items: Vec::new(),
                next: None,
            });
        }

        let candidates: Vec<_> = self
            .visible()
            .iter()
            .filter(|(key, _)| {
                Self::matches(key, start, end, prefix)
                    && after.is_none_or(|last| key.as_str() > last)
            })
            .collect();

        let skip = if after.is_some() { 0 } else { offset };
        let mut items = Vec::new();
        let mut last = None;
        let mut has_more = false;

        for (key, (value, _)) in candidates.into_iter().skip(skip) {
            if items.len() == limit {
                has_more = true;
                break;
            }
            items.push((key.clone(), value.clone()));
            last = Some(key.clone());
        }

        let next = if has_more {
            last.map(|last| Cursor {
                start: start.to_owned(),
                end: end.map(str::to_owned),
                prefix: prefix.map(str::to_owned),
                offset,
                last,
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
        Self::valid_range(start, end)?;

        let mut count = 0usize;
        let mut sum = 0i64;

        for (key, (value, _)) in self.visible() {
            if !Self::matches(key, start, end, prefix) {
                continue;
            }
            let parsed = parse_canonical_i64(value).ok_or_else(|| Error::InvalidNumber {
                key: key.clone(),
                value: value.clone(),
            })?;
            sum = sum
                .checked_add(parsed)
                .ok_or_else(|| Error::SumOverflow { key: key.clone() })?;
            count += 1;
        }

        Ok(Aggregate { count, sum })
    }
}

fn parse_canonical_i64(s: &str) -> Option<i64> {
    let bytes = s.as_bytes();
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
        if bytes[0] == b'-' || digits.len() != 1 {
            return None;
        }
    } else if !(b'1'..=b'9').contains(&digits[0]) {
        return None;
    }

    if !digits.iter().all(u8::is_ascii_digit) {
        return None;
    }

    s.parse::<i64>().ok()
}

fn main() {}