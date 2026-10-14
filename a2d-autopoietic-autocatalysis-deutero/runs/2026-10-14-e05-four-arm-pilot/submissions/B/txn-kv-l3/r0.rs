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
    last_time: u64,
}

impl Store {
    pub fn new() -> Self {
        Self {
            committed: BTreeMap::new(),
            scopes: Vec::new(),
            snapshots: BTreeMap::new(),
            last_time: 0,
        }
    }

    fn admit_and_purge(&mut self, now: u64) -> Result<(), Error> {
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

    fn visible(&self) -> &Data {
        self.scopes.last().unwrap_or(&self.committed)
    }

    fn visible_mut(&mut self) -> &mut Data {
        self.scopes.last_mut().unwrap_or(&mut self.committed)
    }

    fn purge_map(map: &mut Data, now: u64) {
        map.retain(|_, (_, deadline)| deadline.is_none_or(|d| d > now));
    }

    fn purge_visible(&mut self, now: u64) {
        Self::purge_map(self.visible_mut(), now);
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
            && end.is_none_or(|end| key < end)
            && prefix.is_none_or(|prefix| key.starts_with(prefix))
    }

    pub fn begin(&mut self, now: u64) -> Result<(), Error> {
        self.admit_and_purge(now)?;
        if self.scopes.len() >= 4 {
            return Err(Error::DepthLimit);
        }
        self.scopes.push(self.visible().clone());
        Ok(())
    }

    pub fn commit(&mut self, now: u64) -> Result<(), Error> {
        self.admit_and_purge(now)?;
        let Some(map) = self.scopes.pop() else {
            return Err(Error::NoTransaction);
        };
        if let Some(parent) = self.scopes.last_mut() {
            *parent = map;
        } else {
            self.committed = map;
        }
        Ok(())
    }

    pub fn rollback(&mut self, now: u64) -> Result<(), Error> {
        self.admit_and_purge(now)?;
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
        self.admit_and_purge(now)?;
        if key.is_empty() {
            return Err(Error::KeyEmpty);
        }

        let deadline = match ttl {
            None => None,
            Some(0) => {
                self.visible_mut().remove(key);
                return Ok(());
            }
            Some(d) => Some(now.checked_add(d).ok_or(Error::DeadlineOverflow)?),
        };

        self.visible_mut()
            .insert(key.to_owned(), (value.to_owned(), deadline));
        Ok(())
    }

    pub fn delete(&mut self, now: u64, key: &str) -> Result<bool, Error> {
        self.admit_and_purge(now)?;
        if key.is_empty() {
            return Err(Error::KeyEmpty);
        }
        Ok(self.visible_mut().remove(key).is_some())
    }

    pub fn get(&mut self, now: u64, key: &str) -> Result<Option<String>, Error> {
        self.admit_and_purge(now)?;
        if key.is_empty() {
            return Err(Error::KeyEmpty);
        }
        Ok(self.visible().get(key).map(|(value, _)| value.clone()))
    }

    pub fn count(&mut self, now: u64, value: &str) -> Result<usize, Error> {
        self.admit_and_purge(now)?;
        Ok(self
            .visible()
            .values()
            .filter(|(stored, _)| stored == value)
            .count())
    }

    pub fn depth(&mut self, now: u64) -> Result<usize, Error> {
        self.admit_and_purge(now)?;
        Ok(self.scopes.len())
    }

    pub fn snapshot(&mut self, now: u64, name: &str) -> Result<(), Error> {
        self.admit_and_purge(now)?;
        Self::validate_name(name)?;

        if !self.snapshots.contains_key(name) && self.snapshots.len() >= 3 {
            return Err(Error::SnapshotLimit);
        }
        self.snapshots.insert(name.to_owned(), self.visible().clone());
        Ok(())
    }

    pub fn restore(&mut self, now: u64, name: &str) -> Result<(), Error> {
        self.admit_and_purge(now)?;
        Self::validate_name(name)?;
        let Some(mut map) = self.snapshots.get(name).cloned() else {
            return Err(Error::SnapshotMissing(name.to_owned()));
        };
        Self::purge_map(&mut map, now);
        *self.visible_mut() = map;
        Ok(())
    }

    pub fn forget(&mut self, now: u64, name: &str) -> Result<(), Error> {
        self.admit_and_purge(now)?;
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
        self.admit_and_purge(now)?;
        if end.is_some_and(|end| end <= start) {
            return Err(Error::InvalidRange);
        }

        if let Some(cursor) = cursor {
            if cursor.start != start
                || cursor.end.as_deref() != end
                || cursor.prefix.as_deref() != prefix
                || cursor.offset != offset
                || cursor.last.is_empty()
                || !Self::matches(&cursor.last, start, end, prefix)
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
        let mut skipped = 0usize;
        let mut has_more = false;

        for (key, (value, _)) in self.visible() {
            if !Self::matches(key, start, end, prefix) {
                continue;
            }
            if cursor.is_some_and(|c| key <= &c.last) {
                continue;
            }
            if cursor.is_none() && skipped < offset {
                skipped += 1;
                continue;
            }
            if items.len() < limit {
                items.push((key.clone(), value.clone()));
            } else {
                has_more = true;
                break;
            }
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
        self.admit_and_purge(now)?;
        if end.is_some_and(|end| end <= start) {
            return Err(Error::InvalidRange);
        }

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