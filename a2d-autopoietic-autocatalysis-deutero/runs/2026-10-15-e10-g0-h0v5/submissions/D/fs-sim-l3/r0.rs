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

#[derive(Debug, Clone)]
enum Entry {
    Dir,
    File(Vec<u8>),
}

pub struct Fs {
    entries: BTreeMap<String, Entry>,
    cwd: String,
}

impl Default for Fs {
    fn default() -> Self {
        Self::new()
    }
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
        if let Some(i) = path.as_bytes().iter().position(|&b| b == 0) {
            return Err(Error::InvalidPath(i));
        }
        if path.is_empty() {
            return Err(Error::InvalidPath(0));
        }

        let absolute = path.starts_with('/');
        let mut parts: Vec<String> = if absolute {
            Vec::new()
        } else {
            Self::components(&self.cwd)
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

        Ok(Self::make_path(&parts))
    }

    fn components(path: &str) -> Vec<String> {
        path.split('/')
            .filter(|part| !part.is_empty())
            .map(str::to_string)
            .collect()
    }

    fn make_path(parts: &[String]) -> String {
        if parts.is_empty() {
            "/".to_string()
        } else {
            format!("/{}", parts.join("/"))
        }
    }

    fn prefix_path(parts: &[String], count: usize) -> String {
        Self::make_path(&parts[..count])
    }

    fn resolve_parent(&self, path: &str) -> Result<(String, String), Error> {
        let parts = Self::components(path);
        if parts.is_empty() {
            return Err(Error::Root);
        }
        let parent = Self::make_path(&parts[..parts.len() - 1]);
        self.ensure_directory(&parent)?;
        Ok((parent, parts[parts.len() - 1].clone()))
    }

    fn ensure_directory(&self, path: &str) -> Result<(), Error> {
        let parts = Self::components(path);
        for count in 1..=parts.len() {
            let prefix = Self::prefix_path(&parts, count);
            match self.entries.get(&prefix) {
                None => return Err(Error::Missing(prefix)),
                Some(Entry::Dir) => {}
                Some(Entry::File(_)) => return Err(Error::NotDir(prefix)),
            }
        }
        Ok(())
    }

    fn lookup(&self, path: &str) -> Result<&Entry, Error> {
        let parts = Self::components(path);
        for count in 1..parts.len() {
            let prefix = Self::prefix_path(&parts, count);
            match self.entries.get(&prefix) {
                None => return Err(Error::Missing(prefix)),
                Some(Entry::Dir) => {}
                Some(Entry::File(_)) => return Err(Error::NotDir(prefix)),
            }
        }
        match self.entries.get(path) {
            Some(entry) => Ok(entry),
            None => Err(Error::Missing(path.to_string())),
        }
    }

    fn is_within(path: &str, directory: &str) -> bool {
        if directory == "/" {
            return path.starts_with('/');
        }
        path == directory
            || path
                .strip_prefix(directory)
                .is_some_and(|suffix| suffix.starts_with('/'))
    }

    fn has_children(&self, path: &str) -> bool {
        self.entries
            .keys()
            .any(|entry| entry != path && Self::is_within(entry, path))
    }

    pub fn mkdir(&mut self, path: &str) -> Result<(), Error> {
        let resolved = self.validate_and_normalize(path)?;
        let (parent, name) = self.resolve_parent(&resolved)?;
        let child = if parent == "/" {
            format!("/{name}")
        } else {
            format!("{parent}/{name}")
        };
        if self.entries.contains_key(&child) {
            return Err(Error::Exists(child));
        }
        self.entries.insert(child, Entry::Dir);
        Ok(())
    }

    pub fn write(&mut self, path: &str, data: &[u8]) -> Result<(), Error> {
        let resolved = self.validate_and_normalize(path)?;
        if resolved == "/" {
            return Err(Error::IsDir(resolved));
        }
        let (parent, name) = self.resolve_parent(&resolved)?;
        let child = if parent == "/" {
            format!("/{name}")
        } else {
            format!("{parent}/{name}")
        };
        match self.entries.get(&child) {
            Some(Entry::Dir) => Err(Error::IsDir(child)),
            Some(Entry::File(_)) | None => {
                self.entries
                    .insert(child, Entry::File(data.to_vec()));
                Ok(())
            }
        }
    }

    pub fn read(&self, path: &str) -> Result<Vec<u8>, Error> {
        let resolved = self.validate_and_normalize(path)?;
        match self.lookup(&resolved)? {
            Entry::Dir => Err(Error::IsDir(resolved)),
            Entry::File(data) => Ok(data.clone()),
        }
    }

    pub fn ls(&self, path: &str) -> Result<Vec<(String, Kind)>, Error> {
        let resolved = self.validate_and_normalize(path)?;
        match self.lookup(&resolved)? {
            Entry::File(_) => return Err(Error::NotDir(resolved)),
            Entry::Dir => {}
        }

        let prefix = if resolved == "/" {
            "/".to_string()
        } else {
            format!("{resolved}/")
        };
        let mut children = Vec::new();

        for (entry_path, entry) in &self.entries {
            if let Some(rest) = entry_path.strip_prefix(&prefix) {
                if !rest.is_empty() && !rest.contains('/') {
                    let kind = match entry {
                        Entry::Dir => Kind::Dir,
                        Entry::File(_) => Kind::File,
                    };
                    children.push((rest.to_string(), kind));
                }
            }
        }

        children.sort_by(|a, b| a.0.cmp(&b.0));
        Ok(children)
    }

    pub fn cwd(&self) -> String {
        self.cwd.clone()
    }

    pub fn chdir(&mut self, path: &str) -> Result<(), Error> {
        let resolved = self.validate_and_normalize(path)?;
        match self.lookup(&resolved)? {
            Entry::Dir => {
                self.cwd = resolved;
                Ok(())
            }
            Entry::File(_) => Err(Error::NotDir(resolved)),
        }
    }

    pub fn rename(
        &mut self,
        from: &str,
        to: &str,
        overwrite: bool,
    ) -> Result<(), Error> {
        let source = self.validate_and_normalize(from)?;
        let destination = self.validate_and_normalize(to)?;

        if source == "/" || destination == "/" {
            return Err(Error::Root);
        }

        let (source_parent, source_name) = self.resolve_parent(&source)?;
        let source_path = if source_parent == "/" {
            format!("/{source_name}")
        } else {
            format!("{source_parent}/{source_name}")
        };
        let source_entry = self
            .entries
            .get(&source_path)
            .cloned()
            .ok_or_else(|| Error::Missing(source_path.clone()))?;

        if matches!(source_entry, Entry::Dir) && Self::is_within(&self.cwd, &source_path) {
            return Err(Error::Busy(source_path));
        }

        let (destination_parent, destination_name) = self.resolve_parent(&destination)?;
        let destination_path = if destination_parent == "/" {
            format!("/{destination_name}")
        } else {
            format!("{destination_parent}/{destination_name}")
        };

        if matches!(self.entries.get(&destination_path), Some(Entry::Dir))
            && Self::is_within(&self.cwd, &destination_path)
        {
            return Err(Error::Busy(destination_path));
        }

        if source_path == destination_path {
            return Ok(());
        }

        if matches!(source_entry, Entry::Dir)
            && Self::is_within(&destination_path, &source_path)
        {
            return Err(Error::Cycle(destination_path));
        }

        let destination_entry = self.entries.get(&destination_path).cloned();
        if destination_entry.is_some() && !overwrite {
            return Err(Error::Exists(destination_path));
        }

        if let Some(ref existing) = destination_entry {
            match (&source_entry, existing) {
                (Entry::Dir, Entry::File(_)) => {
                    return Err(Error::NotDir(destination_path));
                }
                (Entry::File(_), Entry::Dir) => {
                    return Err(Error::IsDir(destination_path));
                }
                (Entry::Dir, Entry::Dir) if self.has_children(&destination_path) => {
                    return Err(Error::NotEmpty(destination_path));
                }
                _ => {}
            }
        }

        let moving: Vec<(String, Entry)> = self
            .entries
            .iter()
            .filter(|(path, _)| {
                **path == source_path || Self::is_within(path, &source_path)
            })
            .map(|(path, entry)| (path.clone(), entry.clone()))
            .collect();

        if let Some(existing) = destination_entry {
            if matches!(existing, Entry::Dir) {
                self.entries.remove(&destination_path);
            } else {
                self.entries.remove(&destination_path);
            }
        }

        for (old_path, _) in &moving {
            self.entries.remove(old_path);
        }

        for (old_path, entry) in moving {
            let suffix = &old_path[source_path.len()..];
            let new_path = format!("{destination_path}{suffix}");
            self.entries.insert(new_path, entry);
        }

        Ok(())
    }

    pub fn remove(&mut self, path: &str, recursive: bool) -> Result<(), Error> {
        let resolved = self.validate_and_normalize(path)?;
        if resolved == "/" {
            return Err(Error::Root);
        }

        let (parent, name) = self.resolve_parent(&resolved)?;
        let entry_path = if parent == "/" {
            format!("/{name}")
        } else {
            format!("{parent}/{name}")
        };
        let entry = self
            .entries
            .get(&entry_path)
            .ok_or_else(|| Error::Missing(entry_path.clone()))?;

        if matches!(entry, Entry::Dir) && Self::is_within(&self.cwd, &entry_path) {
            return Err(Error::Busy(entry_path));
        }

        if matches!(entry, Entry::Dir) && !recursive && self.has_children(&entry_path) {
            return Err(Error::NotEmpty(entry_path));
        }

        let to_remove: Vec<String> = self
            .entries
            .keys()
            .filter(|candidate| {
                **candidate == entry_path || (recursive && Self::is_within(candidate, &entry_path))
            })
            .cloned()
            .collect();
        for candidate in to_remove {
            self.entries.remove(&candidate);
        }

        Ok(())
    }
}