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
        entries.insert("/".to_owned(), Entry::Dir);
        Self {
            entries,
            cwd: "/".to_owned(),
        }
    }

    fn normalize(&self, path: &str) -> Result<String, Error> {
        if let Some(index) = path.as_bytes().iter().position(|&byte| byte == 0) {
            return Err(Error::InvalidPath(index));
        }
        if path.is_empty() {
            return Err(Error::InvalidPath(0));
        }

        let mut components = if path.starts_with('/') {
            Vec::new()
        } else {
            self.cwd
                .split('/')
                .filter(|component| !component.is_empty())
                .map(str::to_owned)
                .collect()
        };

        for component in path.split('/') {
            match component {
                "" | "." => {}
                ".." => {
                    components.pop();
                }
                other => components.push(other.to_owned()),
            }
        }

        if components.is_empty() {
            Ok("/".to_owned())
        } else {
            Ok(format!("/{}", components.join("/")))
        }
    }

    fn parent_path(path: &str) -> Option<String> {
        if path == "/" {
            return None;
        }
        match path.rfind('/') {
            Some(0) => Some("/".to_owned()),
            Some(index) => Some(path[..index].to_owned()),
            None => Some("/".to_owned()),
        }
    }

    fn lookup_parent(&self, path: &str) -> Result<(), Error> {
        let mut prefix = String::new();
        let components: Vec<&str> = path
            .split('/')
            .filter(|component| !component.is_empty())
            .collect();

        for component in components.iter().take(components.len().saturating_sub(1)) {
            prefix.push('/');
            prefix.push_str(component);
            match self.entries.get(&prefix) {
                None => return Err(Error::Missing(prefix)),
                Some(Entry::Dir) => {}
                Some(Entry::File(_)) => return Err(Error::NotDir(prefix)),
            }
        }

        let parent = Self::parent_path(path).unwrap_or_else(|| "/".to_owned());
        match self.entries.get(&parent) {
            None => Err(Error::Missing(parent)),
            Some(Entry::Dir) => Ok(()),
            Some(Entry::File(_)) => Err(Error::NotDir(parent)),
        }
    }

    pub fn mkdir(&mut self, path: &str) -> Result<(), Error> {
        let path = self.normalize(path)?;
        if path == "/" {
            return Err(Error::Exists(path));
        }

        self.lookup_parent(&path)?;
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

        self.lookup_parent(&path)?;
        match self.entries.get(&path) {
            Some(Entry::Dir) => Err(Error::IsDir(path)),
            Some(Entry::File(_)) | None => {
                self.entries.insert(path, Entry::File(data.to_vec()));
                Ok(())
            }
        }
    }

    pub fn read(&self, path: &str) -> Result<Vec<u8>, Error> {
        let path = self.normalize(path)?;
        if path == "/" {
            return Err(Error::IsDir(path));
        }

        self.lookup_parent(&path)?;
        match self.entries.get(&path) {
            None => Err(Error::Missing(path)),
            Some(Entry::Dir) => Err(Error::IsDir(path)),
            Some(Entry::File(data)) => Ok(data.clone()),
        }
    }

    pub fn ls(&self, path: &str) -> Result<Vec<(String, Kind)>, Error> {
        let path = self.normalize(path)?;

        if path != "/" {
            self.lookup_parent(&path)?;
        }

        match self.entries.get(&path) {
            None => Err(Error::Missing(path)),
            Some(Entry::File(_)) => Err(Error::NotDir(path)),
            Some(Entry::Dir) => {
                let prefix = if path == "/" {
                    "/".to_owned()
                } else {
                    format!("{path}/")
                };
                let mut children = Vec::new();
                for (entry_path, entry) in &self.entries {
                    if let Some(remainder) = entry_path.strip_prefix(&prefix) {
                        if !remainder.is_empty() && !remainder.contains('/') {
                            let kind = match entry {
                                Entry::Dir => Kind::Dir,
                                Entry::File(_) => Kind::File,
                            };
                            children.push((remainder.to_owned(), kind));
                        }
                    }
                }
                children.sort_by(|left, right| left.0.cmp(&right.0));
                Ok(children)
            }
        }
    }

    pub fn cwd(&self) -> String {
        self.cwd.clone()
    }

    pub fn chdir(&mut self, path: &str) -> Result<(), Error> {
        let path = self.normalize(path)?;
        if path != "/" {
            self.lookup_parent(&path)?;
        }

        match self.entries.get(&path) {
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