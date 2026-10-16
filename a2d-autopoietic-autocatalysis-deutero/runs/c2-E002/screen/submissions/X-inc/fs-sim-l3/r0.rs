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
                _ => parts.push(component),
            }
        }

        if parts.is_empty() {
            Ok("/".to_string())
        } else {
            Ok(format!("/{}", parts.join("/")))
        }
    }

    fn child_path(parent: &str, name: &str) -> String {
        if parent == "/" {
            format!("/{name}")
        } else {
            format!("{parent}/{name}")
        }
    }

    fn parent_and_name(path: &str) -> (&str, &str) {
        match path.rfind('/') {
            Some(0) => ("/", &path[1..]),
            Some(i) => (&path[..i], &path[i + 1..]),
            None => ("/", path),
        }
    }

    fn resolve_parent(&self, path: &str) -> Result<(), Error> {
        let (parent, _) = Self::parent_and_name(path);
        self.resolve_existing(parent)?;
        match self.entries.get(parent) {
            Some(Entry::Dir) => Ok(()),
            Some(Entry::File(_)) => Err(Error::NotDir(parent.to_string())),
            None => Err(Error::Missing(parent.to_string())),
        }
    }

    fn resolve_existing(&self, path: &str) -> Result<(), Error> {
        if path == "/" {
            return Ok(());
        }

        let mut prefix = String::new();
        let mut components = path[1..].split('/').peekable();
        while let Some(component) = components.next() {
            prefix.push('/');
            prefix.push_str(component);
            match self.entries.get(&prefix) {
                None => return Err(Error::Missing(prefix)),
                Some(Entry::File(_)) if components.peek().is_some() => {
                    return Err(Error::NotDir(prefix));
                }
                Some(_) => {}
            }
        }
        Ok(())
    }

    fn contains_path(parent: &str, path: &str) -> bool {
        parent == path
            || (parent == "/" && path.starts_with('/'))
            || (parent != "/" && path.strip_prefix(parent).is_some_and(|tail| tail.starts_with('/')))
    }

    fn is_directory(&self, path: &str) -> bool {
        matches!(self.entries.get(path), Some(Entry::Dir))
    }

    fn subtree_keys(&self, path: &str) -> Vec<String> {
        self.entries
            .keys()
            .filter(|candidate| Self::contains_path(path, candidate))
            .cloned()
            .collect()
    }

    pub fn mkdir(&mut self, path: &str) -> Result<(), Error> {
        let path = self.normalize(path)?;
        self.resolve_parent(&path)?;
        if self.entries.contains_key(&path) {
            return Err(Error::Exists(path));
        }
        self.entries.insert(path, Entry::Dir);
        Ok(())
    }

    pub fn write(&mut self, path: &str, data: &[u8]) -> Result<(), Error> {
        let path = self.normalize(path)?;
        self.resolve_parent(&path)?;
        match self.entries.get_mut(&path) {
            Some(Entry::Dir) => Err(Error::IsDir(path)),
            Some(Entry::File(contents)) => {
                contents.clear();
                contents.extend_from_slice(data);
                Ok(())
            }
            None => {
                self.entries.insert(path, Entry::File(data.to_vec()));
                Ok(())
            }
        }
    }

    pub fn read(&self, path: &str) -> Result<Vec<u8>, Error> {
        let path = self.normalize(path)?;
        self.resolve_existing(&path)?;
        match self.entries.get(&path) {
            Some(Entry::File(contents)) => Ok(contents.clone()),
            Some(Entry::Dir) => Err(Error::IsDir(path)),
            None => Err(Error::Missing(path)),
        }
    }

    pub fn ls(&self, path: &str) -> Result<Vec<(String, Kind)>, Error> {
        let path = self.normalize(path)?;
        self.resolve_existing(&path)?;
        match self.entries.get(&path) {
            Some(Entry::File(_)) => return Err(Error::NotDir(path)),
            Some(Entry::Dir) => {}
            None => return Err(Error::Missing(path)),
        }

        let mut children = Vec::new();
        for (candidate, entry) in &self.entries {
            if candidate == &path {
                continue;
            }
            let (parent, name) = Self::parent_and_name(candidate);
            if parent == path {
                let kind = match entry {
                    Entry::Dir => Kind::Dir,
                    Entry::File(_) => Kind::File,
                };
                children.push((name.to_string(), kind));
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
        self.resolve_existing(&path)?;
        match self.entries.get(&path) {
            Some(Entry::Dir) => {
                self.cwd = path;
                Ok(())
            }
            Some(Entry::File(_)) => Err(Error::NotDir(path)),
            None => Err(Error::Missing(path)),
        }
    }

    pub fn rename(&mut self, from: &str, to: &str, overwrite: bool) -> Result<(), Error> {
        let source = self.normalize(from)?;
        let destination = self.normalize(to)?;

        if source == "/" || destination == "/" {
            return Err(Error::Root);
        }

        self.resolve_parent(&source)?;
        let source_entry = self
            .entries
            .get(&source)
            .cloned()
            .ok_or_else(|| Error::Missing(source.clone()))?;

        if matches!(source_entry, Entry::Dir) && Self::contains_path(&source, &self.cwd) {
            return Err(Error::Busy(source));
        }

        self.resolve_parent(&destination)?;
        if matches!(self.entries.get(&destination), Some(Entry::Dir))
            && Self::contains_path(&destination, &self.cwd)
        {
            return Err(Error::Busy(destination));
        }

        if source == destination {
            return Ok(());
        }

        if matches!(source_entry, Entry::Dir) && Self::contains_path(&source, &destination) {
            return Err(Error::Cycle(destination));
        }

        let destination_entry = self.entries.get(&destination).cloned();
        if destination_entry.is_some() && !overwrite {
            return Err(Error::Exists(destination));
        }

        if overwrite {
            match (&source_entry, &destination_entry) {
                (Entry::Dir, Some(Entry::File(_))) => {
                    return Err(Error::NotDir(destination));
                }
                (Entry::File(_), Some(Entry::Dir)) => {
                    return Err(Error::IsDir(destination));
                }
                (Entry::Dir, Some(Entry::Dir)) => {
                    if self
                        .entries
                        .keys()
                        .any(|candidate| candidate != &destination && Self::contains_path(&destination, candidate))
                    {
                        return Err(Error::NotEmpty(destination));
                    }
                }
                _ => {}
            }
        }

        let source_keys = self.subtree_keys(&source);
        let moved: Vec<(String, Entry)> = source_keys
            .iter()
            .filter_map(|key| {
                self.entries
                    .get(key)
                    .cloned()
                    .map(|entry| (key.clone(), entry))
            })
            .collect();

        for key in &source_keys {
            self.entries.remove(key);
        }
        if destination_entry.is_some() {
            for key in self.subtree_keys(&destination) {
                self.entries.remove(&key);
            }
        }
        for (old_path, entry) in moved {
            let suffix = &old_path[source.len()..];
            let new_path = format!("{destination}{suffix}");
            self.entries.insert(new_path, entry);
        }
        Ok(())
    }

    pub fn remove(&mut self, path: &str, recursive: bool) -> Result<(), Error> {
        let path = self.normalize(path)?;
        if path == "/" {
            return Err(Error::Root);
        }

        self.resolve_parent(&path)?;
        let entry = self
            .entries
            .get(&path)
            .cloned()
            .ok_or_else(|| Error::Missing(path.clone()))?;

        if matches!(entry, Entry::Dir) && Self::contains_path(&path, &self.cwd) {
            return Err(Error::Busy(path));
        }

        if matches!(entry, Entry::Dir)
            && !recursive
            && self
                .entries
                .keys()
                .any(|candidate| candidate != &path && Self::contains_path(&path, candidate))
        {
            return Err(Error::NotEmpty(path));
        }

        for key in self.subtree_keys(&path) {
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