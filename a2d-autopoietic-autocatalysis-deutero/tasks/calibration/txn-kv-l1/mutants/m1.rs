use std::collections::BTreeMap;

pub struct Store {
    data: BTreeMap<String, String>,
}

impl Store {
    pub fn new() -> Self {
        Self {
            data: BTreeMap::new(),
        }
    }
    pub fn set(&mut self, key: &str, value: &str) {
        self.data
            .entry(key.to_owned())
            .or_insert_with(|| value.to_owned());
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
}
