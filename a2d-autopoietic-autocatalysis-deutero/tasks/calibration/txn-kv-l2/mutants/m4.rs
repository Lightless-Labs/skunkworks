use std::collections::BTreeMap;

pub struct Store {
    data: BTreeMap<String, String>,
    frames: Vec<BTreeMap<String, String>>,
}

#[derive(Debug, PartialEq, Eq)]
pub enum Error {
    NoTransaction,
}

impl Store {
    pub fn new() -> Self {
        Self {
            data: BTreeMap::new(),
            frames: Vec::new(),
        }
    }
    pub fn set(&mut self, key: &str, value: &str) {
        self.data.insert(key.to_owned(), value.to_owned());
    }
    pub fn get(&self, key: &str) -> Option<String> {
        self.data.get(key).cloned()
    }
    pub fn delete(&mut self, key: &str) -> bool {
        self.data.remove(key).is_some()
    }
    pub fn count(&self, value: &str) -> usize {
        self.data.values().filter(|v| v.as_str() == value).count()
    }
    pub fn begin(&mut self) {
        self.frames.push(self.data.clone());
    }
    pub fn commit(&mut self) -> Result<(), Error> {
        if self.frames.is_empty() {
            return Ok(());
        }
        self.frames.clear();
        Ok(())
    }
    pub fn rollback(&mut self) -> Result<(), Error> {
        self.data = self.frames.pop().ok_or(Error::NoTransaction)?;
        Ok(())
    }
}
