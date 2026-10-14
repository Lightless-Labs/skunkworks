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
#[derive(Clone)]
struct Entry {
    value: String,
    deadline: Option<u64>,
}
type Map = BTreeMap<String, Entry>;
pub struct Store {
    base: Map,
    scopes: Vec<Map>,
    snapshots: BTreeMap<String, Map>,
    clock: u64,
}
impl Store {
    pub fn new() -> Self {
        Self {
            base: Map::new(),
            scopes: Vec::new(),
            snapshots: BTreeMap::new(),
            clock: 0,
        }
    }
    fn visible(&self) -> &Map {
        self.scopes.last().unwrap_or(&self.base)
    }
    fn visible_mut(&mut self) -> &mut Map {
        self.scopes.last_mut().unwrap_or(&mut self.base)
    }
    fn purge(map: &mut Map, now: u64) {
        map.retain(|_, e| e.deadline.is_none_or(|d| d > now));
    }
    fn admit(&mut self, now: u64) -> Result<(), Error> {
        if now < self.clock {
            return Err(Error::TimeBackwards {
                previous: self.clock,
                now,
            });
        }
        self.clock = now;
        Self::purge(self.visible_mut(), now);
        Ok(())
    }
    fn key(key: &str) -> Result<(), Error> {
        if key.is_empty() {
            Err(Error::KeyEmpty)
        } else {
            Ok(())
        }
    }
    fn name(name: &str) -> Result<(), Error> {
        if name.is_empty() {
            return Err(Error::NameEmpty);
        }
        for (at, c) in name.char_indices() {
            if !(c.is_ascii_lowercase() || (at > 0 && (c.is_ascii_digit() || c == '_'))) {
                return Err(Error::NameInvalid { at });
            }
        }
        Ok(())
    }
    pub fn begin(&mut self, now: u64) -> Result<(), Error> {
        self.admit(now)?;
        if self.scopes.len() == 4 {
            return Err(Error::DepthLimit);
        }
        self.scopes.push(self.visible().clone());
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
        self.scopes.pop().ok_or(Error::NoTransaction)?;
        Self::purge(self.visible_mut(), now);
        Ok(())
    }
    pub fn set(&mut self, now: u64, key: &str, value: &str, ttl: Option<u64>) -> Result<(), Error> {
        self.admit(now)?;
        Self::key(key)?;
        let deadline = match ttl {
            None => None,
            Some(d) => Some(now.checked_add(d).ok_or(Error::DeadlineOverflow)?),
        };
        if ttl == Some(0) {
            self.visible_mut().remove(key);
        } else {
            self.visible_mut().insert(
                key.into(),
                Entry {
                    value: value.into(),
                    deadline,
                },
            );
        }
        Ok(())
    }
    pub fn delete(&mut self, now: u64, key: &str) -> Result<bool, Error> {
        self.admit(now)?;
        Self::key(key)?;
        Ok(self.visible_mut().remove(key).is_some())
    }
    pub fn get(&mut self, now: u64, key: &str) -> Result<Option<String>, Error> {
        self.admit(now)?;
        Self::key(key)?;
        Ok(self.visible().get(key).map(|e| e.value.clone()))
    }
    pub fn count(&mut self, now: u64, value: &str) -> Result<usize, Error> {
        self.admit(now)?;
        Ok(self.visible().values().filter(|e| e.value == value).count())
    }
    pub fn depth(&mut self, now: u64) -> Result<usize, Error> {
        self.admit(now)?;
        Ok(self.scopes.len())
    }
    pub fn snapshot(&mut self, now: u64, name: &str) -> Result<(), Error> {
        self.admit(now)?;
        Self::name(name)?;
        if !self.snapshots.contains_key(name) && self.snapshots.len() == 3 {
            return Err(Error::SnapshotLimit);
        }
        self.snapshots.insert(name.into(), self.visible().clone());
        Ok(())
    }
    pub fn restore(&mut self, now: u64, name: &str) -> Result<(), Error> {
        self.admit(now)?;
        Self::name(name)?;
        let mut map = self
            .snapshots
            .get(name)
            .ok_or_else(|| Error::SnapshotMissing(name.into()))?
            .clone();
        Self::purge(&mut map, now);
        *self.visible_mut() = map;
        Ok(())
    }
    pub fn forget(&mut self, now: u64, name: &str) -> Result<(), Error> {
        self.admit(now)?;
        Self::name(name)?;
        self.snapshots
            .remove(name)
            .ok_or_else(|| Error::SnapshotMissing(name.into()))?;
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
        page(self.visible(), start, end, prefix, offset, limit, cursor)
    }
    pub fn aggregate(
        &mut self,
        now: u64,
        start: &str,
        end: Option<&str>,
        prefix: Option<&str>,
    ) -> Result<Aggregate, Error> {
        self.admit(now)?;
        total(self.visible(), start, end, prefix)
    }
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
fn range_ok(start: &str, end: Option<&str>) -> Result<(), Error> {
    if end.is_some_and(|e| e <= start) {
        Err(Error::InvalidRange)
    } else {
        Ok(())
    }
}
fn matches(key: &str, start: &str, end: Option<&str>, prefix: Option<&str>) -> bool {
    key >= start && end.is_none_or(|e| key < e) && prefix.is_none_or(|p| key.starts_with(p))
}
fn page(
    map: &Map,
    start: &str,
    end: Option<&str>,
    prefix: Option<&str>,
    offset: usize,
    limit: usize,
    cursor: Option<&Cursor>,
) -> Result<Page, Error> {
    range_ok(start, end)?;
    if let Some(c) = cursor {
        if c.start != start
            || c.end.as_deref() != end
            || c.prefix.as_deref() != prefix
            || c.offset != offset
            || c.last.is_empty()
            || !matches(&c.last, start, end, prefix)
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
    let skip = if cursor.is_some() { 0 } else { offset };
    let mut iter = map
        .iter()
        .filter(|(k, _)| {
            matches(k, start, end, prefix) && cursor.is_none_or(|c| k.as_str() > c.last.as_str())
        })
        .skip(skip);
    let items: Vec<_> = iter
        .by_ref()
        .take(limit)
        .map(|(k, e)| (k.clone(), e.value.clone()))
        .collect();
    let next = if !items.is_empty() && iter.next().is_some() {
        Some(Cursor {
            start: start.into(),
            end: end.map(str::to_string),
            prefix: prefix.map(str::to_string),
            offset,
            last: items.last().unwrap().0.clone(),
        })
    } else {
        None
    };
    Ok(Page { items, next })
}
fn number(value: &str) -> Option<i64> {
    let bytes = value.as_bytes();
    if value == "0" {
        return Some(0);
    }
    let digits = if bytes.first() == Some(&b'-') {
        &bytes[1..]
    } else {
        bytes
    };
    if digits.is_empty()
        || !(b'1'..=b'9').contains(&digits[0])
        || !digits.iter().all(u8::is_ascii_digit)
    {
        return None;
    }
    value.parse().ok()
}
fn total(
    map: &Map,
    start: &str,
    end: Option<&str>,
    prefix: Option<&str>,
) -> Result<Aggregate, Error> {
    range_ok(start, end)?;
    let mut out = Aggregate { count: 0, sum: 0 };
    for (key, e) in map.iter().filter(|(k, _)| matches(k, start, end, prefix)) {
        let n = number(&e.value).ok_or_else(|| Error::InvalidNumber {
            key: key.clone(),
            value: e.value.clone(),
        })?;
        out.sum = out.sum.wrapping_add(n);
        out.count += 1;
    }
    Ok(out)
}
