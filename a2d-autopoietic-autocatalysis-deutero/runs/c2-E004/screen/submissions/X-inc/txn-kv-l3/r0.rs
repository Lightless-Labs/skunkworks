use std::collections::{BTreeMap, HashMap};

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

#[derive(Clone, Default)]
struct Data {
    entries: BTreeMap<String, Entry>,
}

impl Data {
    fn purge(&mut self, now: u64) {
        self.entries
            .retain(|_, entry| entry.deadline.is_none_or(|deadline| deadline > now));
    }

    fn live_copy(&self) -> Self {
        self.clone()
    }
}

pub struct Store {
    committed: Data,
    scopes: Vec<Data>,
    snapshots: HashMap<String, Data>,
    clock: u64,
}

impl Store {
    pub fn new() -> Self {
        Self {
            committed: Data::default(),
            scopes: Vec::new(),
            snapshots: HashMap::new(),
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
        self.visible_mut().purge(now);
        Ok(())
    }

    fn visible(&self) -> &Data {
        self.scopes.last().unwrap_or(&self.committed)
    }

    fn visible_mut(&mut self) -> &mut Data {
        self.scopes.last_mut().unwrap_or(&mut self.committed)
    }

    pub fn begin(&mut self, now: u64) -> Result<(), Error> {
        self.admit(now)?;
        if self.scopes.len() >= 4 {
            return Err(Error::DepthLimit);
        }
        self.scopes.push(self.visible().live_copy());
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
        self.visible_mut().purge(now);
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
                self.visible_mut().entries.remove(key);
                return Ok(());
            }
            Some(duration) => Some(now.checked_add(duration).ok_or(Error::DeadlineOverflow)?),
        };

        self.visible_mut().entries.insert(
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
        Ok(self.visible_mut().entries.remove(key).is_some())
    }

    pub fn get(&mut self, now: u64, key: &str) -> Result<Option<String>, Error> {
        self.admit(now)?;
        if key.is_empty() {
            return Err(Error::KeyEmpty);
        }
        Ok(self
            .visible()
            .entries
            .get(key)
            .map(|entry| entry.value.clone()))
    }

    pub fn count(&mut self, now: u64, value: &str) -> Result<usize, Error> {
        self.admit(now)?;
        Ok(self
            .visible()
            .entries
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
        self.snapshots
            .insert(name.to_owned(), self.visible().live_copy());
        Ok(())
    }

    pub fn restore(&mut self, now: u64, name: &str) -> Result<(), Error> {
        self.admit(now)?;
        Self::validate_name(name)?;
        let Some(mut data) = self.snapshots.get(name).cloned() else {
            return Err(Error::SnapshotMissing(name.to_owned()));
        };
        data.purge(now);
        *self.visible_mut() = data;
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

    fn validate_name(name: &str) -> Result<(), Error> {
        if name.is_empty() {
            return Err(Error::NameEmpty);
        }

        for (at, ch) in name.char_indices() {
            let valid = if at == 0 {
                ch.is_ascii_lowercase()
            } else {
                ch.is_ascii_lowercase() || ch.is_ascii_digit() || ch == '_'
            };
            if !valid {
                return Err(Error::NameInvalid { at });
            }
        }
        Ok(())
    }

    fn matches(key: &str, start: &str, end: Option<&str>, prefix: Option<&str>) -> bool {
        key >= start
            && end.is_none_or(|bound| key < bound)
            && prefix.is_none_or(|p| key.starts_with(p))
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

        let after = if let Some(cursor) = cursor {
            if cursor.start != start
                || cursor.end.as_deref() != end
                || cursor.prefix.as_deref() != prefix
                || cursor.offset != offset
                || cursor.last.is_empty()
                || !Self::matches(&cursor.last, start, end, prefix)
            {
                return Err(Error::CursorMismatch);
            }
            Some(cursor.last.as_str())
        } else {
            None
        };

        let mut matching = self.visible().entries.iter().filter(|(key, _)| {
            Self::matches(key, start, end, prefix)
                && after.is_none_or(|last| key.as_str() > last)
        });

        if after.is_none() {
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
        if limit > 0 {
            for (key, entry) in matching.by_ref().take(limit) {
                items.push((key.clone(), entry.value.clone()));
            }
        }

        let has_more = !items.is_empty() && matching.next().is_some();
        let next = if has_more {
            let last = items.last().unwrap().0.clone();
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
        if end.is_some_and(|bound| bound <= start) {
            return Err(Error::InvalidRange);
        }

        let mut count = 0usize;
        let mut sum = 0i64;

        for (key, entry) in self.visible().entries.iter().filter(|(key, _)| {
            Self::matches(key, start, end, prefix)
        }) {
            let parsed = Self::parse_number(&entry.value).ok_or_else(|| Error::InvalidNumber {
                key: key.clone(),
                value: entry.value.clone(),
            })?;
            sum = sum.checked_add(parsed).ok_or_else(|| Error::SumOverflow {
                key: key.clone(),
            })?;
            count += 1;
        }

        Ok(Aggregate { count, sum })
    }

    fn parse_number(value: &str) -> Option<i64> {
        if value == "0" {
            return Some(0);
        }

        let digits = value.strip_prefix('-').unwrap_or(value);
        if digits.is_empty()
            || digits.starts_with('0')
            || !digits.bytes().all(|byte| byte.is_ascii_digit())
        {
            return None;
        }

        let parsed = value.parse::<i64>().ok()?;
        if parsed == 0 && value == "-0" {
            return None;
        }
        Some(parsed)
    }
}