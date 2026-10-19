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

#[derive(Debug, Clone, PartialEq, Eq)]
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
        entries.insert("/".to_owned(), Entry::Dir);
        Self {
            entries,
            cwd: "/".to_owned(),
        }
    }

    fn normalize(&self, path: &str) -> Result<String, Error> {
        if let Some(i) = path.as_bytes().iter().position(|&b| b == 0) {
            return Err(Error::InvalidPath(i));
        }
        if path.is_empty() {
            return Err(Error::InvalidPath(0));
        }

        let mut parts: Vec<&str> = Vec::new();
        if !path.starts_with('/') && self.cwd != "/" {
            parts.extend(self.cwd[1..].split('/'));
        }

        for component in path.split('/') {
            match component {
                "" | "." => {}
                ".." => {
                    parts.pop();
                }
                other => parts.push(other),
            }
        }

        if parts.is_empty() {
            Ok("/".to_owned())
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

    fn name(path: &str) -> &str {
        path.rsplit('/').next().unwrap_or("")
    }

    fn resolve_parent(&self, path: &str) -> Result<(), Error> {
        let parent = Self::parent_path(path);
        self.resolve_directory(parent)
    }

    fn resolve_directory(&self, path: &str) -> Result<(), Error> {
        if path == "/" {
            return Ok(());
        }

        let mut prefix = String::new();
        for component in path[1..].split('/') {
            prefix.push('/');
            prefix.push_str(component);
            match self.entries.get(&prefix) {
                None => return Err(Error::Missing(prefix)),
                Some(Entry::File(_)) => return Err(Error::NotDir(prefix)),
                Some(Entry::Dir) => {}
            }
        }
        Ok(())
    }

    fn resolve_entry(&self, path: &str) -> Result<Option<&Entry>, Error> {
        if path == "/" {
            return Ok(self.entries.get("/"));
        }

        let parent = Self::parent_path(path);
        self.resolve_directory(parent)?;
        Ok(self.entries.get(path))
    }

    pub fn mkdir(&mut self, path: &str) -> Result<(), Error> {
        let path = self.normalize(path)?;
        if path == "/" {
            return Err(Error::Exists(path));
        }

        self.resolve_parent(&path)?;
        if self.entries.contains_key(&path) {
            return Err(Error::Exists(path));
        }

        self.entries.insert(path, Entry::Dir);
        Ok(())
    }

    pub fn write(&mut self, path: &str, data: &[u8]) -> Result<(), Error> {
        let path = self.normalize(path)?;
        if path == "/" {
            return Err(Error::IsDir(path));
        }

        self.resolve_parent(&path)?;
        if matches!(self.entries.get(&path), Some(Entry::Dir)) {
            return Err(Error::IsDir(path));
        }

        self.entries.insert(path, Entry::File(data.to_vec()));
        Ok(())
    }

    pub fn read(&self, path: &str) -> Result<Vec<u8>, Error> {
        let path = self.normalize(path)?;
        match self.resolve_entry(&path)? {
            None => Err(Error::Missing(path)),
            Some(Entry::Dir) => Err(Error::IsDir(path)),
            Some(Entry::File(data)) => Ok(data.clone()),
        }
    }

    pub fn ls(&self, path: &str) -> Result<Vec<(String, Kind)>, Error> {
        let path = self.normalize(path)?;
        match self.resolve_entry(&path)? {
            None => return Err(Error::Missing(path)),
            Some(Entry::File(_)) => return Err(Error::NotDir(path)),
            Some(Entry::Dir) => {}
        }

        let mut children = Vec::new();
        for (entry_path, entry) in &self.entries {
            if entry_path == "/" {
                continue;
            }
            if Self::parent_path(entry_path) == path {
                let kind = match entry {
                    Entry::Dir => Kind::Dir,
                    Entry::File(_) => Kind::File,
                };
                children.push((Self::name(entry_path).to_owned(), kind));
            }
        }
        children.sort_by(|a, b| a.0.cmp(&b.0));
        Ok(children)
    }

    pub fn cwd(&self) -> String {
        self.cwd.clone()
    }

    pub fn chdir(&mut self, path: &str) -> Result<(), Error> {
        let path = self.normalize(path)?;
        match self.resolve_entry(&path)? {
            None => Err(Error::Missing(path)),
            Some(Entry::File(_)) => Err(Error::NotDir(path)),
            Some(Entry::Dir) => {
                self.cwd = path;
                Ok(())
            }
        }
    }
}

impl Default for Fs {
    fn default() -> Self {
        Self::new()
    }
}