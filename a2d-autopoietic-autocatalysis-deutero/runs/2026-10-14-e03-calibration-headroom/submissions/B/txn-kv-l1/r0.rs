use std::collections::HashMap;

pub struct Store {
    values: HashMap<String, String>,
}

impl Store {
    pub fn new() -> Self {
        Self {
            values: HashMap::new(),
        }
    }

    pub fn set(&mut self, key: &str, value: &str) {
        self.values.insert(key.to_owned(), value.to_owned());
    }

    pub fn get(&self, key: &str) -> Option<String> {
        self.values.get(key).cloned()
    }

    pub fn delete(&mut self, key: &str) -> bool {
        self.values.remove(key).is_some()
    }

    pub fn count(&self, value: &str) -> usize {
        self.values.values().filter(|stored| stored.as_str() == value).count()
    }
}