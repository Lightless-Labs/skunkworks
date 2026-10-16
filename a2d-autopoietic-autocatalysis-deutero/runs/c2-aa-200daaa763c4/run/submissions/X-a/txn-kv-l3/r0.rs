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

type Data = BTreeMap<String, Entry>;

#[derive(Clone)]
struct Entry {
    value: String,
    deadline: Option<u64>,
}

impl Entry {
    fn live_at(&self, now: u64) -> bool {
        self.deadline.map_or(true, |deadline| deadline > now)
    }
}

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

    fn visible(&self) -> &Data {
        self.scopes.last().unwrap_or(&self.committed)
    }

    fn visible_mut(&mut self) -> &mut Data {
        self.scopes.last_mut().unwrap_or(&mut self.committed)
    }

    fn purge(data: &mut Data, now: u64) {
        data.retain(|_, entry| entry.live_at(now));
    }

    fn purge_visible(&mut self, now: u64) {
        Self::purge(self.visible_mut(), now);
    }

    fn valid_name(name: &str) -> Result<(), Error> {
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
            && end.map_or(true, |bound| key < bound)
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
        if key.is_empty() {
            return Err(Error::KeyEmpty);
        }

        let deadline = match ttl {
            None => None,
            Some(duration) => match now.checked_add(duration) {
                Some(deadline) => Some(deadline),
                None => return Err(Error::DeadlineOverflow),
            },
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
        self.admit(now)?;
        if key.is_empty() {
            return Err(Error::KeyEmpty);
        }
        Ok(self.visible_mut().remove(key).is_some())
    }

    pub fn get(&mut self, now: u64, key: &str) -> Result<Option<String>, Error> {
        self.admit(now)?;
        if key.is_empty() {
            return Err(Error::KeyEmpty);
        }
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
        Self::valid_name(name)?;
        if !self.snapshots.contains_key(name) && self.snapshots.len() >= 3 {
            return Err(Error::SnapshotLimit);
        }
        self.snapshots.insert(name.to_owned(), self.visible().clone());
        Ok(())
    }

    pub fn restore(&mut self, now: u64, name: &str) -> Result<(), Error> {
        self.admit(now)?;
        Self::valid_name(name)?;
        let Some(mut data) = self.snapshots.get(name).cloned() else {
            return Err(Error::SnapshotMissing(name.to_owned()));
        };
        Self::purge(&mut data, now);
        *self.visible_mut() = data;
        Ok(())
    }

    pub fn forget(&mut self, now: u64, name: &str) -> Result<(), Error> {
        self.admit(now)?;
        Self::valid_name(name)?;
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
        if end.is_some_and(|bound| bound <= start) {
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

        for (key, entry) in self.visible() {
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
                items.push((key.clone(), entry.value.clone()));
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
        self.admit(now)?;
        if end.is_some_and(|bound| bound <= start) {
            return Err(Error::InvalidRange);
        }

        let mut count = 0usize;
        let mut sum = 0i64;

        for (key, entry) in self.visible() {
            if !Self::matches(key, start, end, prefix) {
                continue;
            }
            let value = parse_canonical_i64(&entry.value).ok_or_else(|| {
                Error::InvalidNumber {
                    key: key.clone(),
                    value: entry.value.clone(),
                }
            })?;
            sum = sum
                .checked_add(value)
                .ok_or_else(|| Error::SumOverflow { key: key.clone() })?;
            count += 1;
        }

        Ok(Aggregate { count, sum })
    }
}

fn parse_canonical_i64(text: &str) -> Option<i64> {
    let bytes = text.as_bytes();
    if bytes.is_empty() {
        return None;
    }

    let (negative, digits) = if bytes[0] == b'-' {
        (true, &bytes[1..])
    } else {
        (false, bytes)
    };

    if digits.is_empty()
        || !digits.iter().all(u8::is_ascii_digit)
        || (digits.len() > 1 && digits[0] == b'0')
        || (negative && digits == b"0")
    {
        return None;
    }

    text.parse::<i64>().ok()
}