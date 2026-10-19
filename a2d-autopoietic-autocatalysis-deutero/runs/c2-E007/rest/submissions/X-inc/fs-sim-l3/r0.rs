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

impl Fs {
    pub fn new() -> Self {
        let mut entries = BTreeMap::new();
        entries.insert("/".to_string(), Entry::Dir);
        Self {
            entries,
            cwd: "/".to_string(),
        }
    }

    fn canonical(parts: &[String]) -> String {
        if parts.is_empty() {
            "/".to_string()
        } else {
            format!("/{}", parts.join("/"))
        }
    }

    fn normalize(&self, path: &str) -> Result<String, Error> {
        if let Some(i) = path.as_bytes().iter().position(|&b| b == 0) {
            return Err(Error::InvalidPath(i));
        }
        if path.is_empty() {
            return Err(Error::InvalidPath(0));
        }

        let mut parts: Vec<String> = if path.starts_with('/') {
            Vec::new()
        } else if self.cwd == "/" {
            Vec::new()
        } else {
            self.cwd[1..].split('/').map(str::to_string).collect()
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
        Ok(Self::canonical(&parts))
    }

    fn parent(path: &str) -> String {
        match path.rfind('/') {
            Some(0) | None => "/".to_string(),
            Some(i) => path[..i].to_string(),
        }
    }

    fn is_within(path: &str, dir: &str) -> bool {
        if dir == "/" {
            path.starts_with('/')
        } else {
            path == dir
                || path
                    .strip_prefix(dir)
                    .is_some_and(|suffix| suffix.starts_with('/'))
        }
    }

    fn resolve_dir(&self, path: &str) -> Result<(), Error> {
        let mut current = String::from("/");
        let components: Vec<&str> = path
            .strip_prefix('/')
            .unwrap_or(path)
            .split('/')
            .filter(|part| !part.is_empty())
            .collect();

        for (index, component) in components.iter().enumerate() {
            current = if current == "/" {
                format!("/{component}")
            } else {
                format!("{current}/{component}")
            };
            match self.entries.get(&current) {
                None => return Err(Error::Missing(current)),
                Some(Entry::Dir) => {}
                Some(Entry::File(_)) if index + 1 < components.len() => {
                    return Err(Error::NotDir(current));
                }
                Some(Entry::File(_)) => return Err(Error::NotDir(current)),
            }
        }
        Ok(())
    }

    fn resolve_parent(&self, path: &str) -> Result<String, Error> {
        let parent = Self::parent(path);
        self.resolve_dir(&parent)?;
        Ok(parent)
    }

    fn descendants(&self, path: &str) -> Vec<String> {
        self.entries
            .keys()
            .filter(|candidate| *candidate != path && Self::is_within(candidate, path))
            .cloned()
            .collect()
    }

    pub fn mkdir(&mut self, path: &str) -> Result<(), Error> {
        let path = self.normalize(path)?;
        let parent = self.resolve_parent(&path)?;
        if self.entries.contains_key(&path) {
            return Err(Error::Exists(path));
        }
        self.entries.insert(path, Entry::Dir);
        let _ = parent;
        Ok(())
    }

    pub fn write(&mut self, path: &str, data: &[u8]) -> Result<(), Error> {
        let path = self.normalize(path)?;
        if path == "/" {
            return Err(Error::IsDir(path));
        }
        self.resolve_parent(&path)?;
        match self.entries.get_mut(&path) {
            Some(Entry::Dir) => Err(Error::IsDir(path)),
            Some(Entry::File(bytes)) => {
                bytes.clear();
                bytes.extend_from_slice(data);
                Ok(())
            }
            None => {
                self.entries
                    .insert(path, Entry::File(data.to_vec()));
                Ok(())
            }
        }
    }

    pub fn read(&self, path: &str) -> Result<Vec<u8>, Error> {
        let path = self.normalize(path)?;
        if path == "/" {
            return Err(Error::IsDir(path));
        }
        self.resolve_parent(&path)?;
        match self.entries.get(&path) {
            None => Err(Error::Missing(path)),
            Some(Entry::Dir) => Err(Error::IsDir(path)),
            Some(Entry::File(bytes)) => Ok(bytes.clone()),
        }
    }

    pub fn ls(&self, path: &str) -> Result<Vec<(String, Kind)>, Error> {
        let path = self.normalize(path)?;
        self.resolve_dir(&path)?;
        let prefix = if path == "/" {
            "/".to_string()
        } else {
            format!("{path}/")
        };
        let mut result = Vec::new();
        for (candidate, entry) in &self.entries {
            if candidate == &path || !candidate.starts_with(&prefix) {
                continue;
            }
            let remainder = &candidate[prefix.len()..];
            if remainder.contains('/') {
                continue;
            }
            let kind = match entry {
                Entry::Dir => Kind::Dir,
                Entry::File(_) => Kind::File,
            };
            result.push((remainder.to_string(), kind));
        }
        result.sort_by(|a, b| a.0.cmp(&b.0));
        Ok(result)
    }

    pub fn cwd(&self) -> String {
        self.cwd.clone()
    }

    pub fn chdir(&mut self, path: &str) -> Result<(), Error> {
        let path = self.normalize(path)?;
        self.resolve_dir(&path)?;
        match self.entries.get(&path) {
            Some(Entry::Dir) => {
                self.cwd = path;
                Ok(())
            }
            _ => Err(Error::NotDir(path)),
        }
    }

    pub fn rename(
        &mut self,
        from: &str,
        to: &str,
        overwrite: bool,
    ) -> Result<(), Error> {
        let from = self.normalize(from)?;
        let to = self.normalize(to)?;

        if from == "/" || to == "/" {
            return Err(Error::Root);
        }

        let source_parent = self.resolve_parent(&from)?;
        let source = self
            .entries
            .get(&from)
            .cloned()
            .ok_or_else(|| Error::Missing(from.clone()))?;

        if matches!(source, Entry::Dir) && Self::is_within(&self.cwd, &from) {
            return Err(Error::Busy(from));
        }

        let destination_parent = self.resolve_parent(&to)?;

        if matches!(self.entries.get(&to), Some(Entry::Dir))
            && Self::is_within(&self.cwd, &to)
        {
            return Err(Error::Busy(to));
        }

        if from == to {
            return Ok(());
        }

        if matches!(source, Entry::Dir) && Self::is_within(&to, &from) {
            return Err(Error::Cycle(to));
        }

        let destination = self.entries.get(&to).cloned();
        if destination.is_some() && !overwrite {
            return Err(Error::Exists(to));
        }

        if let Some(dest_entry) = &destination {
            match (&source, dest_entry) {
                (Entry::Dir, Entry::File(_)) => return Err(Error::NotDir(to)),
                (Entry::File(_), Entry::Dir) => return Err(Error::IsDir(to)),
                (Entry::Dir, Entry::Dir) => {
                    if !self.descendants(&to).is_empty() {
                        return Err(Error::NotEmpty(to));
                    }
                }
                _ => {}
            }
        }

        let _ = (source_parent, destination_parent);
        let source_keys: Vec<String> = self
            .entries
            .keys()
            .filter(|candidate| *candidate == &from || Self::is_within(candidate, &from))
            .cloned()
            .collect();

        let moved: Vec<(String, Entry)> = source_keys
            .iter()
            .filter_map(|old| {
                let suffix = &old[from.len()..];
                let new = format!("{to}{suffix}");
                self.entries.get(old).cloned().map(|entry| (new, entry))
            })
            .collect();

        if destination.is_some() {
            for key in self
                .entries
                .keys()
                .filter(|candidate| *candidate == &to || Self::is_within(candidate, &to))
                .cloned()
                .collect::<Vec<_>>()
            {
                self.entries.remove(&key);
            }
        }

        for key in source_keys {
            self.entries.remove(&key);
        }
        for (key, entry) in moved {
            self.entries.insert(key, entry);
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

        if matches!(entry, Entry::Dir) && Self::is_within(&self.cwd, &path) {
            return Err(Error::Busy(path));
        }

        let descendants = self.descendants(&path);
        if matches!(entry, Entry::Dir) && !recursive && !descendants.is_empty() {
            return Err(Error::NotEmpty(path));
        }

        self.entries.remove(&path);
        if recursive {
            for descendant in descendants {
                self.entries.remove(&descendant);
            }
        }
        Ok(())
    }
}

impl Default for Fs {
    fn default() -> Self {
        Self::new()
    }
}