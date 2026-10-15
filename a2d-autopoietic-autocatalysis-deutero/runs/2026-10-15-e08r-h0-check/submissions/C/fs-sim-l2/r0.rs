use std::collections::BTreeMap;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    Dir,
    File,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Error {
    InvalidPath(usize),
    Missing(String),
    NotDir(String),
    IsDir(String),
    Exists(String),
}

#[derive(Debug, Clone)]
enum Entry {
    Dir,
    File(Vec<u8>),
}

pub struct Fs {
    entries: BTreeMap<String, Entry>,
    cwd: Vec<String>,
}

impl Fs {
    pub fn new() -> Self {
        let mut entries = BTreeMap::new();
        entries.insert("/".to_string(), Entry::Dir);
        Self {
            entries,
            cwd: Vec::new(),
        }
    }

    fn canonical(parts: &[String]) -> String {
        if parts.is_empty() {
            "/".to_string()
        } else {
            format!("/{}", parts.join("/"))
        }
    }

    fn normalize(&self, path: &str) -> Result<Vec<String>, Error> {
        if let Some(i) = path.as_bytes().iter().position(|&b| b == 0) {
            return Err(Error::InvalidPath(i));
        }
        if path.is_empty() {
            return Err(Error::InvalidPath(0));
        }

        let mut parts = if path.starts_with('/') {
            Vec::new()
        } else {
            self.cwd.clone()
        };
        for component in path.split('/') {
            match component {
                "" | "." => {}
                ".." => {
                    parts.pop();
                }
                other => parts.push(other.to_string()),
            }
        }
        Ok(parts)
    }

    fn lookup(&self, parts: &[String]) -> Result<&Entry, Error> {
        let mut prefix = Vec::new();
        for (index, component) in parts.iter().enumerate() {
            let parent_path = Self::canonical(&prefix);
            match self.entries.get(&parent_path) {
                Some(Entry::Dir) => {}
                Some(Entry::File(_)) => return Err(Error::NotDir(parent_path)),
                None => return Err(Error::Missing(parent_path)),
            }

            prefix.push(component.clone());
            let path = Self::canonical(&prefix);
            match self.entries.get(&path) {
                Some(entry) if index + 1 == parts.len() => return Ok(entry),
                Some(Entry::Dir) => {}
                Some(Entry::File(_)) => return Err(Error::NotDir(path)),
                None => return Err(Error::Missing(path)),
            }
        }
        self.entries
            .get("/")
            .ok_or_else(|| Error::Missing("/".to_string()))
    }

    fn check_parent(&self, parts: &[String]) -> Result<(), Error> {
        let parent_path = Self::canonical(&parts[..parts.len() - 1]);
        match self.entries.get(&parent_path) {
            Some(Entry::Dir) => Ok(()),
            Some(Entry::File(_)) => Err(Error::NotDir(parent_path)),
            None => Err(Error::Missing(parent_path)),
        }
    }

    pub fn mkdir(&mut self, path: &str) -> Result<(), Error> {
        let parts = self.normalize(path)?;
        if parts.is_empty() {
            return Err(Error::Exists("/".to_string()));
        }
        self.check_parent(&parts)?;
        let full_path = Self::canonical(&parts);
        if self.entries.contains_key(&full_path) {
            return Err(Error::Exists(full_path));
        }
        self.entries.insert(full_path, Entry::Dir);
        Ok(())
    }

    pub fn write(&mut self, path: &str, data: &[u8]) -> Result<(), Error> {
        let parts = self.normalize(path)?;
        if parts.is_empty() {
            return Err(Error::IsDir("/".to_string()));
        }
        self.check_parent(&parts)?;
        let full_path = Self::canonical(&parts);
        match self.entries.get_mut(&full_path) {
            Some(Entry::Dir) => Err(Error::IsDir(full_path)),
            Some(Entry::File(contents)) => {
                *contents = data.to_vec();
                Ok(())
            }
            None => {
                self.entries.insert(full_path, Entry::File(data.to_vec()));
                Ok(())
            }
        }
    }

    pub fn read(&self, path: &str) -> Result<Vec<u8>, Error> {
        let parts = self.normalize(path)?;
        match self.lookup(&parts)? {
            Entry::Dir => Err(Error::IsDir(Self::canonical(&parts))),
            Entry::File(contents) => Ok(contents.clone()),
        }
    }

    pub fn ls(&self, path: &str) -> Result<Vec<(String, Kind)>, Error> {
        let parts = self.normalize(path)?;
        let full_path = Self::canonical(&parts);
        match self.lookup(&parts)? {
            Entry::File(_) => return Err(Error::NotDir(full_path.clone())),
            Entry::Dir => {}
        }

        let mut children = Vec::new();
        for (entry_path, entry) in &self.entries {
            if entry_path == "/" {
                continue;
            }
            let Some((parent, basename)) = entry_path.rsplit_once('/') else {
                continue;
            };
            let parent = if parent.is_empty() { "/" } else { parent };
            if parent == full_path {
                let kind = match entry {
                    Entry::Dir => Kind::Dir,
                    Entry::File(_) => Kind::File,
                };
                children.push((basename.to_string(), kind));
            }
        }
        children.sort_by(|a, b| a.0.cmp(&b.0));
        Ok(children)
    }

    pub fn cwd(&self) -> String {
        Self::canonical(&self.cwd)
    }

    pub fn chdir(&mut self, path: &str) -> Result<(), Error> {
        let parts = self.normalize(path)?;
        match self.lookup(&parts)? {
            Entry::Dir => {
                self.cwd = parts;
                Ok(())
            }
            Entry::File(_) => Err(Error::NotDir(Self::canonical(&parts))),
        }
    }
}

fn main() {}