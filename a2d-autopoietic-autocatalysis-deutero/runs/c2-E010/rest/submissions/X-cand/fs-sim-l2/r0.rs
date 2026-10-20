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
    cwd: String,
}

impl Fs {
    pub fn new() -> Self {
        let mut entries = BTreeMap::new();
        entries.insert("/".to_string(), Entry::Dir);
        Self {
            entries,
            cwd: "/".to_string(),
        }
    }

    fn normalize(&self, path: &str) -> Result<String, Error> {
        if let Some(index) = path.as_bytes().iter().position(|&b| b == 0) {
            return Err(Error::InvalidPath(index));
        }
        if path.is_empty() {
            return Err(Error::InvalidPath(0));
        }

        let mut parts: Vec<&str> = Vec::new();
        if !path.starts_with('/') {
            parts.extend(self.cwd.split('/').filter(|part| !part.is_empty()));
        }

        for part in path.split('/') {
            match part {
                "" | "." => {}
                ".." => {
                    parts.pop();
                }
                other => parts.push(other),
            }
        }

        if parts.is_empty() {
            Ok("/".to_string())
        } else {
            let mut result = String::new();
            for part in parts {
                result.push('/');
                result.push_str(part);
            }
            Ok(result)
        }
    }

    fn parent_path(path: &str) -> &str {
        match path.rfind('/') {
            Some(0) | None => "/",
            Some(index) => &path[..index],
        }
    }

    fn lookup_parent(&self, path: &str) -> Result<(), Error> {
        let parent = Self::parent_path(path);
        let mut prefix = String::new();

        for component in parent.split('/').filter(|part| !part.is_empty()) {
            prefix.push('/');
            prefix.push_str(component);
            match self.entries.get(&prefix) {
                None => return Err(Error::Missing(prefix)),
                Some(Entry::Dir) => {}
                Some(Entry::File(_)) => return Err(Error::NotDir(prefix)),
            }
        }
        Ok(())
    }

    fn lookup_full(&self, path: &str) -> Result<&Entry, Error> {
        let mut prefix = String::new();
        let components: Vec<&str> = path.split('/').filter(|part| !part.is_empty()).collect();

        for (index, component) in components.iter().enumerate() {
            prefix.push('/');
            prefix.push_str(component);
            match self.entries.get(&prefix) {
                None => return Err(Error::Missing(prefix)),
                Some(entry @ Entry::File(_)) if index + 1 < components.len() => {
                    let _ = entry;
                    return Err(Error::NotDir(prefix));
                }
                Some(entry) if index + 1 == components.len() => return Ok(entry),
                Some(Entry::Dir) => {}
                Some(Entry::File(_)) => unreachable!(),
            }
        }

        self.entries.get("/").ok_or_else(|| Error::Missing("/".to_string()))
    }

    pub fn mkdir(&mut self, path: &str) -> Result<(), Error> {
        let canonical = self.normalize(path)?;
        if canonical == "/" {
            return Err(Error::Exists(canonical));
        }
        self.lookup_parent(&canonical)?;
        if self.entries.contains_key(&canonical) {
            return Err(Error::Exists(canonical));
        }
        self.entries.insert(canonical, Entry::Dir);
        Ok(())
    }

    pub fn write(&mut self, path: &str, data: &[u8]) -> Result<(), Error> {
        let canonical = self.normalize(path)?;
        if canonical == "/" {
            return Err(Error::IsDir(canonical));
        }
        self.lookup_parent(&canonical)?;
        match self.entries.get(&canonical) {
            Some(Entry::Dir) => Err(Error::IsDir(canonical)),
            Some(Entry::File(_)) => {
                self.entries
                    .insert(canonical, Entry::File(data.to_vec()));
                Ok(())
            }
            None => {
                self.entries
                    .insert(canonical, Entry::File(data.to_vec()));
                Ok(())
            }
        }
    }

    pub fn read(&self, path: &str) -> Result<Vec<u8>, Error> {
        let canonical = self.normalize(path)?;
        match self.lookup_full(&canonical)? {
            Entry::Dir => Err(Error::IsDir(canonical)),
            Entry::File(bytes) => Ok(bytes.clone()),
        }
    }

    pub fn ls(&self, path: &str) -> Result<Vec<(String, Kind)>, Error> {
        let canonical = self.normalize(path)?;
        match self.lookup_full(&canonical)? {
            Entry::File(_) => return Err(Error::NotDir(canonical)),
            Entry::Dir => {}
        }

        let mut children = Vec::new();
        let prefix = if canonical == "/" {
            "/".to_string()
        } else {
            format!("{}/", canonical)
        };

        for (entry_path, entry) in &self.entries {
            if !entry_path.starts_with(&prefix) || entry_path == &canonical {
                continue;
            }
            let remainder = &entry_path[prefix.len()..];
            if remainder.is_empty() || remainder.contains('/') {
                continue;
            }
            let kind = match entry {
                Entry::Dir => Kind::Dir,
                Entry::File(_) => Kind::File,
            };
            children.push((remainder.to_string(), kind));
        }
        children.sort_by(|a, b| a.0.cmp(&b.0));
        Ok(children)
    }

    pub fn cwd(&self) -> String {
        self.cwd.clone()
    }

    pub fn chdir(&mut self, path: &str) -> Result<(), Error> {
        let canonical = self.normalize(path)?;
        match self.lookup_full(&canonical)? {
            Entry::Dir => {
                self.cwd = canonical;
                Ok(())
            }
            Entry::File(_) => Err(Error::NotDir(canonical)),
        }
    }
}

impl Default for Fs {
    fn default() -> Self {
        Self::new()
    }
}
