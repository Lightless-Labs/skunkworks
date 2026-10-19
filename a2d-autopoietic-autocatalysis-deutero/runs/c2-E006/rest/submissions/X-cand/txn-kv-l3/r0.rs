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

type Entries = BTreeMap<String, Entry>;

#[derive(Clone)]
struct Entry {
    value: String,
    deadline: Option<u64>,
}

pub struct Store {
    committed: Entries,
    scopes: Vec<Entries>,
    snapshots: BTreeMap<String, Entries>,
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
        Self::purge(Self::visible_mut(&mut self.committed, &mut self.scopes), now);
        Ok(())
    }

    fn visible<'a>(committed: &'a Entries, scopes: &'a [Entries]) -> &'a Entries {
        scopes.last().unwrap_or(committed)
    }

    fn visible_mut<'a>(committed: &'a mut Entries, scopes: &'a mut [Entries]) -> &'a mut Entries {
        scopes.last_mut().unwrap_or(committed)
    }

    fn purge(entries: &mut Entries, now: u64) {
        entries.retain(|_, entry| entry.deadline.map_or(true, |d| d > now));
    }

    fn valid_name(name: &str) -> Result<(), Error> {
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

    fn matches(key: &str, start: &str, end: Option<&str>, prefix: Option<&str>) -> bool {
        key >= start
            && end.map_or(true, |end| key < end)
            && prefix.map_or(true, |prefix| key.starts_with(prefix))
    }

    pub fn begin(&mut self, now: u64) -> Result<(), Error> {
        self.admit(now)?;
        if self.scopes.len() >= 4 {
            return Err(Error::DepthLimit);
        }
        self.scopes.push(Self::visible(&self.committed, &self.scopes).clone());
        Ok(())
    }

    pub fn commit(&mut self, now: u64) -> Result<(), Error> {
        self.admit(now)?;
        let scope = self.scopes.pop().ok_or(Error::NoTransaction)?;
        if let Some(parent) = self.scopes.last_mut() {
            *parent = scope;
        } else {
            self.committed = scope;
        }
        Ok(())
    }

    pub fn rollback(&mut self, now: u64) -> Result<(), Error> {
        self.admit(now)?;
        self.scopes.pop().ok_or(Error::NoTransaction)?;
        Self::purge(Self::visible_mut(&mut self.committed, &mut self.scopes), now);
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
        if key.is_empty() {
            return Err(Error::KeyEmpty);
        }
        let deadline = match ttl {
            None => None,
            Some(0) => {
                Self::visible_mut(&mut self.committed, &mut self.scopes).remove(key);
                return Ok(());
            }
            Some(d) => Some(now.checked_add(d).ok_or(Error::DeadlineOverflow)?),
        };
        Self::visible_mut(&mut self.committed, &mut self.scopes).insert(
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
        if key.is_empty() {
            return Err(Error::KeyEmpty);
        }
        Ok(Self::visible_mut(&mut self.committed, &mut self.scopes)
            .remove(key)
            .is_some())
    }

    pub fn get(&mut self, now: u64, key: &str) -> Result<Option<String>, Error> {
        self.admit(now)?;
        if key.is_empty() {
            return Err(Error::KeyEmpty);
        }
        Ok(Self::visible(&self.committed, &self.scopes)
            .get(key)
            .map(|entry| entry.value.clone()))
    }

    pub fn count(&mut self, now: u64, value: &str) -> Result<usize, Error> {
        self.admit(now)?;
        Ok(Self::visible(&self.committed, &self.scopes)
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
        Self::valid_name(name)?;
        if !self.snapshots.contains_key(name) && self.snapshots.len() >= 3 {
            return Err(Error::SnapshotLimit);
        }
        self.snapshots.insert(
            name.to_owned(),
            Self::visible(&self.committed, &self.scopes).clone(),
        );
        Ok(())
    }

    pub fn restore(&mut self, now: u64, name: &str) -> Result<(), Error> {
        self.admit(now)?;
        Self::valid_name(name)?;
        let mut saved = self
            .snapshots
            .get(name)
            .cloned()
            .ok_or_else(|| Error::SnapshotMissing(name.to_owned()))?;
        Self::purge(&mut saved, now);
        *Self::visible_mut(&mut self.committed, &mut self.scopes) = saved;
        Ok(())
    }

    pub fn forget(&mut self, now: u64, name: &str) -> Result<(), Error> {
        self.admit(now)?;
        Self::valid_name(name)?;
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
        if end.map_or(false, |end| end <= start) {
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

        let entries = Self::visible(&self.committed, &self.scopes);
        let mut items = Vec::new();
        let mut skipped = 0usize;
        let mut has_more = false;

        for (key, entry) in entries {
            if !Self::matches(key, start, end, prefix) {
                continue;
            }
            if let Some(cursor) = cursor {
                if key <= &cursor.last {
                    continue;
                }
            } else if skipped < offset {
                skipped += 1;
                continue;
            }

            if items.len() < limit {
                items.push((key.clone(), entry.value.clone()));
            } else {
                has_more = true;
                break;
            }
        }

        let next = if has_more {
            let last = items.last().expect("has_more implies a nonempty page").0.clone();
            Some(Cursor {
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
        if end.map_or(false, |end| end <= start) {
            return Err(Error::InvalidRange);
        }

        let mut count = 0usize;
        let mut sum = 0i64;
        for (key, entry) in Self::visible(&self.committed, &self.scopes) {
            if !Self::matches(key, start, end, prefix) {
                continue;
            }
            let parsed = parse_canonical_i64(&entry.value).ok_or_else(|| {
                Error::InvalidNumber {
                    key: key.clone(),
                    value: entry.value.clone(),
                }
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

    if digits == b"0" {
        return if bytes[0] == b'-' { None } else { Some(0) };
    }
    if digits.is_empty() || digits[0] == b'0' || !digits[0].is_ascii_digit() {
        return None;
    }
    if !digits.iter().all(u8::is_ascii_digit) {
        return None;
    }

    value.parse::<i64>().ok()
}