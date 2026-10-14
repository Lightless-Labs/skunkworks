use std::collections::HashMap;

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

type Data = HashMap<String, String>;

pub struct Store {
    committed: Data,
    scopes: Vec<Data>,
    snapshots: HashMap<String, Data>,
}

impl Store {
    /// Creates an empty store.
    pub fn new() -> Self {
        Self {
            committed: HashMap::new(),
            scopes: Vec::new(),
            snapshots: HashMap::new(),
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

        for (byte_index, ch) in name.char_indices() {
            let valid = if byte_index == 0 {
                ch.is_ascii_lowercase()
            } else {
                ch.is_ascii_lowercase() || ch.is_ascii_digit() || ch == '_'
            };
            if !valid {
                return Err(Error::NameInvalid { at: byte_index });
            }
        }
        Ok(())
    }

    /// Starts a working scope copied from the currently visible map.
    pub fn begin(&mut self) -> Result<(), Error> {
        if self.scopes.len() >= 4 {
            return Err(Error::DepthLimit);
        }
        self.scopes.push(self.visible().clone());
        Ok(())
    }

    /// Commits the innermost working scope into its parent map.
    pub fn commit(&mut self) -> Result<(), Error> {
        let data = self.scopes.pop().ok_or(Error::NoTransaction)?;
        *self.scopes.last_mut().unwrap_or(&mut self.committed) = data;
        Ok(())
    }

    /// Discards the innermost working scope.
    pub fn rollback(&mut self) -> Result<(), Error> {
        self.scopes.pop().map(|_| ()).ok_or(Error::NoTransaction)
    }

    /// Inserts or overwrites a key in the currently visible map.
    pub fn set(&mut self, key: &str, value: &str) -> Result<(), Error> {
        Self::validate_key(key)?;
        self.visible_mut().insert(key.to_owned(), value.to_owned());
        Ok(())
    }

    /// Removes a key from the visible map and reports whether it existed.
    pub fn delete(&mut self, key: &str) -> Result<bool, Error> {
        Self::validate_key(key)?;
        Ok(self.visible_mut().remove(key).is_some())
    }

    /// Returns an owned copy of the value associated with a nonempty key.
    pub fn get(&mut self, key: &str) -> Result<Option<String>, Error> {
        Self::validate_key(key)?;
        Ok(self.visible().get(key).cloned())
    }

    /// Counts visible keys whose complete value equals the supplied value.
    pub fn count(&mut self, value: &str) -> Result<usize, Error> {
        Ok(self.visible().values().filter(|v| v.as_str() == value).count())
    }

    /// Returns the number of currently open working scopes.
    pub fn depth(&mut self) -> Result<usize, Error> {
        Ok(self.scopes.len())
    }

    /// Saves a copy of the visible map under a valid name.
    pub fn snapshot(&mut self, name: &str) -> Result<(), Error> {
        Self::validate_name(name)?;
        if !self.snapshots.contains_key(name) && self.snapshots.len() >= 3 {
            return Err(Error::SnapshotLimit);
        }
        let data = self.visible().clone();
        self.snapshots.insert(name.to_owned(), data);
        Ok(())
    }

    /// Replaces the visible map with a copy of a named snapshot.
    pub fn restore(&mut self, name: &str) -> Result<(), Error> {
        Self::validate_name(name)?;
        let data = self
            .snapshots
            .get(name)
            .cloned()
            .ok_or_else(|| Error::SnapshotMissing(name.to_owned()))?;
        *self.visible_mut() = data;
        Ok(())
    }

    /// Removes a named snapshot.
    pub fn forget(&mut self, name: &str) -> Result<(), Error> {
        Self::validate_name(name)?;
        self.snapshots
            .remove(name)
            .map(|_| ())
            .ok_or_else(|| Error::SnapshotMissing(name.to_owned()))
    }
}

fn main() {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn basic_values() {
        let mut s = Store::new();
        s.set("red", "7").unwrap();
        s.set("blue", "7").unwrap();
        assert_eq!(s.get("red"), Ok(Some("7".into())));
        assert_eq!(s.count("7"), Ok(2));
        assert_eq!(s.delete("blue"), Ok(true));
        assert_eq!(s.get("blue"), Ok(None));
    }

    #[test]
    fn invalid_inputs_leave_state_unchanged() {
        let mut s = Store::new();
        assert_eq!(s.set("", "x"), Err(Error::KeyEmpty));
        assert_eq!(s.snapshot("Valid"), Err(Error::NameInvalid { at: 0 }));
        assert_eq!(s.depth(), Ok(0));
        assert_eq!(s.count(""), Ok(0));
    }

    #[test]
    fn nested_commit_and_outer_rollback() {
        let mut s = Store::new();
        s.begin().unwrap();
        s.set("q", "a").unwrap();
        s.begin().unwrap();
        s.set("q", "b").unwrap();
        s.commit().unwrap();
        assert_eq!(s.get("q"), Ok(Some("b".into())));
        s.rollback().unwrap();
        assert_eq!(s.get("q"), Ok(None));
    }

    #[test]
    fn snapshot_copy_survives_scope_rollback() {
        let mut s = Store::new();
        s.set("key", "first").unwrap();
        s.begin().unwrap();
        s.set("key", "second").unwrap();
        s.snapshot("saved").unwrap();
        s.rollback().unwrap();
        s.restore("saved").unwrap();
        assert_eq!(s.get("key"), Ok(Some("second".into())));
    }
}