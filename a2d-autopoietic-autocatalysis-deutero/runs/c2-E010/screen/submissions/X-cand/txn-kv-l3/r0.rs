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

#[derive(Clone)]
struct Entry {
    value: String,
    deadline: Option<u64>,
}

type Map = BTreeMap<String, Entry>;

pub struct Store {
    data: Map,
    scopes: Vec<Map>,
    saved: BTreeMap<String, Map>,
    time: u64,
}

impl Store {
    pub fn new() -> Self {
        Self {
            data: Map::new(),
            scopes: Vec::new(),
            saved: BTreeMap::new(),
            time: 0,
        }
    }

    fn visible(&self) -> &Map {
        match self.scopes.last() {
            Some(map) => map,
            None => &self.data,
        }
    }

    fn visible_mut(&mut self) -> &mut Map {
        match self.scopes.last_mut() {
            Some(map) => map,
            None => &mut self.data,
        }
    }

    fn enter_time(&mut self, now: u64) -> Result<(), Error> {
        if now < self.time {
            return Err(Error::TimeBackwards {
                previous: self.time,
                now,
            });
        }
        self.time = now;
        self.purge_visible(now);
        Ok(())
    }

    fn purge_visible(&mut self, now: u64) {
        self.visible_mut()
            .retain(|_, entry| entry.deadline.map_or(true, |t| t > now));
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
        for (i, ch) in name.char_indices() {
            let b = ch as u32;
            let valid = if i == 0 {
                b >= b'a' as u32 && b <= b'z' as u32
            } else {
                (b >= b'a' as u32 && b <= b'z' as u32)
                    || (b >= b'0' as u32 && b <= b'9' as u32)
                    || b == b'_' as u32
            };
            if !valid {
                return Err(Error::NameInvalid { at: i });
            }
        }
        Ok(())
    }

    fn in_range(key: &str, start: &str, end: Option<&str>, prefix: Option<&str>) -> bool {
        key >= start
            && end.map_or(true, |end| key < end)
            && prefix.map_or(true, |prefix| key.starts_with(prefix))
    }

    pub fn begin(&mut self, now: u64) -> Result<(), Error> {
        self.enter_time(now)?;
        if self.scopes.len() == 4 {
            return Err(Error::DepthLimit);
        }
        let next = self.visible().clone();
        self.scopes.push(next);
        Ok(())
    }

    pub fn commit(&mut self, now: u64) -> Result<(), Error> {
        self.enter_time(now)?;
        let completed = match self.scopes.pop() {
            Some(map) => map,
            None => return Err(Error::NoTransaction),
        };
        match self.scopes.last_mut() {
            Some(parent) => *parent = completed,
            None => self.data = completed,
        }
        Ok(())
    }

    pub fn rollback(&mut self, now: u64) -> Result<(), Error> {
        self.enter_time(now)?;
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
        self.enter_time(now)?;
        Self::validate_key(key)?;
        let deadline = match ttl {
            None => None,
            Some(delta) => Some(now.checked_add(delta).ok_or(Error::DeadlineOverflow)?),
        };
        if deadline == Some(now) {
            self.visible_mut().remove(key);
        } else {
            self.visible_mut().insert(
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
        self.enter_time(now)?;
        Self::validate_key(key)?;
        Ok(self.visible_mut().remove(key).is_some())
    }

    pub fn get(&mut self, now: u64, key: &str) -> Result<Option<String>, Error> {
        self.enter_time(now)?;
        Self::validate_key(key)?;
        Ok(self.visible().get(key).map(|entry| entry.value.clone()))
    }

    pub fn count(&mut self, now: u64, value: &str) -> Result<usize, Error> {
        self.enter_time(now)?;
        Ok(self
            .visible()
            .values()
            .filter(|entry| entry.value == value)
            .count())
    }

    pub fn depth(&mut self, now: u64) -> Result<usize, Error> {
        self.enter_time(now)?;
        Ok(self.scopes.len())
    }

    pub fn snapshot(&mut self, now: u64, name: &str) -> Result<(), Error> {
        self.enter_time(now)?;
        Self::validate_name(name)?;
        if !self.saved.contains_key(name) && self.saved.len() == 3 {
            return Err(Error::SnapshotLimit);
        }
        let contents = self.visible().clone();
        self.saved.insert(name.to_owned(), contents);
        Ok(())
    }

    pub fn restore(&mut self, now: u64, name: &str) -> Result<(), Error> {
        self.enter_time(now)?;
        Self::validate_name(name)?;
        let contents = self
            .saved
            .get(name)
            .cloned()
            .ok_or_else(|| Error::SnapshotMissing(name.to_owned()))?;
        *self.visible_mut() = contents;
        self.purge_visible(now);
        Ok(())
    }

    pub fn forget(&mut self, now: u64, name: &str) -> Result<(), Error> {
        self.enter_time(now)?;
        Self::validate_name(name)?;
        match self.saved.remove(name) {
            Some(_) => Ok(()),
            None => Err(Error::SnapshotMissing(name.to_owned())),
        }
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
        self.enter_time(now)?;
        if end.map_or(false, |end| end <= start) {
            return Err(Error::InvalidRange);
        }
        if let Some(c) = cursor {
            if c.start != start
                || c.end.as_deref() != end
                || c.prefix.as_deref() != prefix
                || c.offset != offset
                || c.last.is_empty()
                || !Self::in_range(&c.last, start, end, prefix)
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

        let mut found = Vec::new();
        let mut skipped = 0usize;
        let mut has_more = false;
        for (key, entry) in self.visible() {
            if !Self::in_range(key, start, end, prefix) {
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

            if found.len() == limit {
                has_more = true;
                break;
            }
            found.push((key.clone(), entry.value.clone()));
        }

        let next = if has_more {
            Some(Cursor {
                start: start.to_owned(),
                end: end.map(str::to_owned),
                prefix: prefix.map(str::to_owned),
                offset,
                last: found.last().unwrap().0.clone(),
            })
        } else {
            None
        };
        Ok(Page { items: found, next })
    }

    pub fn aggregate(
        &mut self,
        now: u64,
        start: &str,
        end: Option<&str>,
        prefix: Option<&str>,
    ) -> Result<Aggregate, Error> {
        self.enter_time(now)?;
        if end.map_or(false, |end| end <= start) {
            return Err(Error::InvalidRange);
        }
        let mut result = Aggregate { count: 0, sum: 0 };
        for (key, entry) in self.visible() {
            if !Self::in_range(key, start, end, prefix) {
                continue;
            }
            let number = parse_number(&entry.value).ok_or_else(|| Error::InvalidNumber {
                key: key.clone(),
                value: entry.value.clone(),
            })?;
            result.sum = result
                .sum
                .checked_add(number)
                .ok_or_else(|| Error::SumOverflow { key: key.clone() })?;
            result.count += 1;
        }
        Ok(result)
    }
}

fn parse_number(value: &str) -> Option<i64> {
    let bytes = value.as_bytes();
    if bytes.is_empty() {
        return None;
    }

    let digits = if bytes[0] == b'-' {
        &bytes[1..]
    } else {
        bytes
    };

    if digits.is_empty()
        || !digits.iter().all(|b| b.is_ascii_digit())
        || (digits.len() > 1 && digits[0] == b'0')
        || (bytes[0] == b'-' && digits == b"0")
    {
        return None;
    }

    value.parse::<i64>().ok()
}
