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
    Root,
    Busy(String),
    Cycle(String),
    NotEmpty(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum Entry {
    Dir,
    File(Vec<u8>),
}

impl Entry {
    fn kind(&self) -> Kind {
        match self {
            Entry::Dir => Kind::Dir,
            Entry::File(_) => Kind::File,
        }
    }
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

    fn validate_and_normalize(&self, path: &str) -> Result<String, Error> {
        if let Some(index) = path.as_bytes().iter().position(|&b| b == 0) {
            return Err(Error::InvalidPath(index));
        }
        if path.is_empty() {
            return Err(Error::InvalidPath(0));
        }

        let mut components: Vec<&str> = Vec::new();
        if !path.starts_with('/') {
            components.extend(self.cwd.split('/').filter(|c| !c.is_empty()));
        }

        for component in path.split('/') {
            match component {
                "" | "." => {}
                ".." => {
                    components.pop();
                }
                other => components.push(other),
            }
        }

        if components.is_empty() {
            Ok("/".to_string())
        } else {
            Ok(format!("/{}", components.join("/")))
        }
    }

    fn parent_and_name(path: &str) -> (&str, &str) {
        match path.rfind('/') {
            Some(0) => ("/", &path[1..]),
            Some(index) => (&path[..index], &path[index + 1..]),
            None => ("/", path),
        }
    }

    fn resolve_dir(&self, path: &str) -> Result<(), Error> {
        if path == "/" {
            return Ok(());
        }

        let mut prefix = String::new();
        let components: Vec<&str> = path.split('/').filter(|c| !c.is_empty()).collect();

        for (index, component) in components.iter().enumerate() {
            prefix.push('/');
            prefix.push_str(component);
            match self.entries.get(&prefix) {
                None => return Err(Error::Missing(prefix)),
                Some(Entry::Dir) => {}
                Some(Entry::File(_)) if index + 1 < components.len() => {
                    return Err(Error::NotDir(prefix));
                }
                Some(Entry::File(_)) => return Err(Error::NotDir(prefix)),
            }
        }
        Ok(())
    }

    fn resolve_parent(&self, path: &str) -> Result<(), Error> {
        let (parent, _) = Self::parent_and_name(path);
        self.resolve_dir(parent)
    }

    fn is_within(path: &str, directory: &str) -> bool {
        if directory == "/" {
            return path.starts_with('/');
        }
        path == directory
            || path
                .strip_prefix(directory)
                .is_some_and(|rest| rest.starts_with('/'))
    }

    fn subtree_keys(&self, path: &str) -> Vec<String> {
        self.entries
            .keys()
            .filter(|candidate| Self::is_within(candidate, path))
            .cloned()
            .collect()
    }

    pub fn mkdir(&mut self, path: &str) -> Result<(), Error> {
        let path = self.validate_and_normalize(path)?;
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
        let path = self.validate_and_normalize(path)?;
        if path == "/" {
            return Err(Error::IsDir(path));
        }
        self.resolve_parent(&path)?;
        match self.entries.get(&path) {
            Some(Entry::Dir) => return Err(Error::IsDir(path)),
            Some(Entry::File(_)) | None => {}
        }
        self.entries
            .insert(path, Entry::File(data.to_vec()));
        Ok(())
    }

    pub fn read(&self, path: &str) -> Result<Vec<u8>, Error> {
        let path = self.validate_and_normalize(path)?;
        if path == "/" {
            return Err(Error::IsDir(path));
        }
        self.resolve_parent(&path)?;
        match self.entries.get(&path) {
            None => Err(Error::Missing(path)),
            Some(Entry::Dir) => Err(Error::IsDir(path)),
            Some(Entry::File(data)) => Ok(data.clone()),
        }
    }

    pub fn ls(&self, path: &str) -> Result<Vec<(String, Kind)>, Error> {
        let path = self.validate_and_normalize(path)?;
        self.resolve_parent(&path)?;
        let entry = self
            .entries
            .get(&path)
            .ok_or_else(|| Error::Missing(path.clone()))?;
        if !matches!(entry, Entry::Dir) {
            return Err(Error::NotDir(path));
        }

        let mut children = Vec::new();
        for (candidate, child) in &self.entries {
            if candidate == "/" || candidate == &path {
                continue;
            }
            let (parent, name) = Self::parent_and_name(candidate);
            if parent == path {
                children.push((name.to_string(), child.kind()));
            }
        }
        children.sort_by(|a, b| a.0.cmp(&b.0));
        Ok(children)
    }

    pub fn cwd(&self) -> String {
        self.cwd.clone()
    }

    pub fn chdir(&mut self, path: &str) -> Result<(), Error> {
        let path = self.validate_and_normalize(path)?;
        self.resolve_parent(&path)?;
        match self.entries.get(&path) {
            None => Err(Error::Missing(path)),
            Some(Entry::Dir) => {
                self.cwd = path;
                Ok(())
            }
            Some(Entry::File(_)) => Err(Error::NotDir(path)),
        }
    }

    pub fn rename(&mut self, from: &str, to: &str, overwrite: bool) -> Result<(), Error> {
        let source = self.validate_and_normalize(from)?;
        let destination = self.validate_and_normalize(to)?;

        if source == "/" || destination == "/" {
            return Err(Error::Root);
        }

        self.resolve_parent(&source)?;
        let source_entry = self
            .entries
            .get(&source)
            .cloned()
            .ok_or_else(|| Error::Missing(source.clone()))?;

        if matches!(source_entry, Entry::Dir) && Self::is_within(&self.cwd, &source) {
            return Err(Error::Busy(source));
        }

        self.resolve_parent(&destination)?;
        if matches!(self.entries.get(&destination), Some(Entry::Dir))
            && Self::is_within(&self.cwd, &destination)
        {
            return Err(Error::Busy(destination));
        }

        if source == destination {
            return Ok(());
        }

        if matches!(source_entry, Entry::Dir) && Self::is_within(&destination, &source) {
            return Err(Error::Cycle(destination));
        }

        if let Some(destination_entry) = self.entries.get(&destination) {
            if !overwrite {
                return Err(Error::Exists(destination));
            }

            match (&source_entry, destination_entry) {
                (Entry::Dir, Entry::File(_)) => return Err(Error::NotDir(destination)),
                (Entry::File(_), Entry::Dir) => return Err(Error::IsDir(destination)),
                (Entry::Dir, Entry::Dir) => {
                    if self.entries.keys().any(|candidate| {
                        candidate != &destination && Self::is_within(candidate, &destination)
                    }) {
                        return Err(Error::NotEmpty(destination));
                    }
                }
                (Entry::File(_), Entry::File(_)) => {}
            }
        }

        let source_keys = self.subtree_keys(&source);
        let moved: Vec<(String, Entry)> = source_keys
            .iter()
            .filter_map(|old_path| {
                self.entries.get(old_path).map(|entry| {
                    let suffix = &old_path[source.len()..];
                    (format!("{destination}{suffix}"), entry.clone())
                })
            })
            .collect();

        for key in &source_keys {
            self.entries.remove(key);
        }
        self.entries.retain(|key, _| !Self::is_within(key, &destination));
        for (key, entry) in moved {
            self.entries.insert(key, entry);
        }
        Ok(())
    }

    pub fn remove(&mut self, path: &str, recursive: bool) -> Result<(), Error> {
        let path = self.validate_and_normalize(path)?;
        if path == "/" {
            return Err(Error::Root);
        }

        self.resolve_parent(&path)?;
        let entry = self
            .entries
            .get(&path)
            .cloned()
            .ok_or_else(|| Error::Missing(path.clone()))?;

        if matches!(entry, Entry::Dir) && Self::is_within(&self.cwd, &path) {
            return Err(Error::Busy(path));
        }

        let keys = self.subtree_keys(&path);
        if matches!(entry, Entry::Dir) && !recursive && keys.len() > 1 {
            return Err(Error::NotEmpty(path));
        }

        for key in keys {
            self.entries.remove(&key);
        }
        Ok(())
    }
}

impl Default for Fs {
    fn default() -> Self {
        Self::new()
    }
}