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
}
pub struct Store {
    base: BTreeMap<String, String>,
    scopes: Vec<BTreeMap<String, String>>,
    snapshots: BTreeMap<String, BTreeMap<String, String>>,
}
impl Store {
    pub fn new() -> Self {
        Self {
            base: BTreeMap::new(),
            scopes: Vec::new(),
            snapshots: BTreeMap::new(),
        }
    }
    fn visible(&self) -> &BTreeMap<String, String> {
        self.scopes.last().unwrap_or(&self.base)
    }
    fn visible_mut(&mut self) -> &mut BTreeMap<String, String> {
        self.scopes.last_mut().unwrap_or(&mut self.base)
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
    pub fn begin(&mut self) -> Result<(), Error> {
        if self.scopes.len() == 4 {
            return Err(Error::DepthLimit);
        }
        self.scopes.push(self.visible().clone());
        Ok(())
    }
    pub fn commit(&mut self) -> Result<(), Error> {
        let map = self.scopes.pop().ok_or(Error::NoTransaction)?;
        *self.visible_mut() = map;
        Ok(())
    }
    pub fn rollback(&mut self) -> Result<(), Error> {
        self.scopes.pop().ok_or(Error::NoTransaction)?;
        Ok(())
    }
    pub fn set(&mut self, key: &str, value: &str) -> Result<(), Error> {
        Self::key(key)?;
        self.visible_mut().insert(key.into(), value.into());
        Ok(())
    }
    pub fn delete(&mut self, key: &str) -> Result<bool, Error> {
        Self::key(key)?;
        Ok(self.visible_mut().remove(key).is_some())
    }
    pub fn get(&mut self, key: &str) -> Result<Option<String>, Error> {
        Self::key(key)?;
        Ok(self.visible().get(key).cloned())
    }
    pub fn count(&mut self, value: &str) -> Result<usize, Error> {
        Ok(self
            .visible()
            .values()
            .filter(|v| v.as_str() == value)
            .count())
    }
    pub fn depth(&mut self) -> Result<usize, Error> {
        Ok(self.scopes.len())
    }
    pub fn snapshot(&mut self, name: &str) -> Result<(), Error> {
        Self::name(name)?;
        if !self.snapshots.contains_key(name) && self.snapshots.len() == 3 {
            return Err(Error::SnapshotLimit);
        }
        self.snapshots.insert(name.into(), self.visible().clone());
        Ok(())
    }
    pub fn restore(&mut self, name: &str) -> Result<(), Error> {
        Self::name(name)?;
        let map = self
            .snapshots
            .get(name)
            .ok_or_else(|| Error::SnapshotMissing(name.into()))?
            .clone();
        self.scopes.clear();
        self.base = map;
        Ok(())
    }
    pub fn forget(&mut self, name: &str) -> Result<(), Error> {
        Self::name(name)?;
        self.snapshots
            .remove(name)
            .ok_or_else(|| Error::SnapshotMissing(name.into()))?;
        Ok(())
    }
}
