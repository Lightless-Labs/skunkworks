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

pub struct Store {
    committed: BTreeMap<String, Entry>,
    scopes: Vec<BTreeMap<String, Entry>>,
    snapshots: BTreeMap<String, BTreeMap<String, Entry>>,
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

    fn visible(&self) -> &BTreeMap<String, Entry> {
        self.scopes.last().unwrap_or(&self.committed)
    }

    fn visible_mut(&mut self) -> &mut BTreeMap<String, Entry> {
        match self.scopes.last_mut() {
            Some(map) => map,
            None => &mut self.committed,
        }
    }

    fn purge(map: &mut BTreeMap<String, Entry>, now: u64) {
        map.retain(|_, entry| entry.deadline.map_or(true, |deadline| deadline > now));
    }

    fn purge_visible(&mut self, now: u64) {
        Self::purge(self.visible_mut(), now);
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

    fn matches(key: &str, start: &str, end: Option<&str>, prefix: Option<&str>) -> bool {
        key >= start
            && end.map_or(true, |bound| key < bound)
            && prefix.map_or(true, |prefix| key.starts_with(prefix))
    }

    pub fn begin(&mut self, now: u64) -> Result<(), Error> {
        self.admit(now)?;
        if self.scopes.len() >= 4 {
            return Err(Error::DepthLimit);
        }
        let map = self.visible().clone();
        self.scopes.push(map);
        Ok(())
    }

    pub fn commit(&mut self, now: u64) -> Result<(), Error> {
        self.admit(now)?;
        let map = self.scopes.pop().ok_or(Error::NoTransaction)?;
        *self.visible_mut() = map;
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

        let map = self.visible().clone();
        self.snapshots.insert(name.to_owned(), map);
        Ok(())
    }

    pub fn restore(&mut self, now: u64, name: &str) -> Result<(), Error> {
        self.admit(now)?;
        Self::validate_name(name)?;

        let map = self
            .snapshots
            .get(name)
            .cloned()
            .ok_or_else(|| Error::SnapshotMissing(name.to_owned()))?;
        *self.visible_mut() = map;
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

        if end.map_or(false, |bound| bound <= start) {
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

        let mut matching = self.visible().iter().filter(|(key, _)| {
            Self::matches(key.as_str(), start, end, prefix)
                && cursor.map_or(true, |c| key.as_str() > c.last.as_str())
        });

        if cursor.is_none() {
            for _ in 0..offset {
                if matching.next().is_none() {
                    return Ok(Page {
                        items: Vec::new(),
                        next: None,
                    });
                }
            }
        }

        let mut items = Vec::new();
        for _ in 0..limit {
            match matching.next() {
                Some((key, entry)) => items.push((key.clone(), entry.value.clone())),
                None => break,
            }
        }

        let has_more = !items.is_empty() && matching.next().is_some();
        let next = if has_more {
            Some(Cursor {
                start: start.to_owned(),
                end: end.map(str::to_owned),
                prefix: prefix.map(str::to_owned),
                offset,
                last: items.last().map(|item| item.0.clone()).unwrap_or_default(),
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

        if end.map_or(false, |bound| bound <= start) {
            return Err(Error::InvalidRange);
        }

        let mut count = 0usize;
        let mut sum = 0i64;

        for (key, entry) in self.visible() {
            if !Self::matches(key, start, end, prefix) {
                continue;
            }

            let value = parse_canonical_i64(&entry.value).ok_or_else(|| Error::InvalidNumber {
                key: key.clone(),
                value: entry.value.clone(),
            })?;

            sum = sum
                .checked_add(value)
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

    if digits.iter().any(|byte| !byte.is_ascii_digit()) {
        return None;
    }
    if digits.len() > 1 && digits[0] == b'0' {
        return None;
    }
    if bytes[0] == b'-' && digits == b"0" {
        return None;
    }

    value.parse::<i64>().ok()
}