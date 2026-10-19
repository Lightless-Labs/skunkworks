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

    fn normalize(&self, path: &str) -> Result<String, Error> {
        if let Some(i) = path.as_bytes().iter().position(|&b| b == 0) {
            return Err(Error::InvalidPath(i));
        }
        if path.is_empty() {
            return Err(Error::InvalidPath(0));
        }

        let mut parts: Vec<&str> = Vec::new();
        if !path.starts_with('/') {
            parts.extend(self.cwd.split('/').filter(|s| !s.is_empty()));
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
            Ok("/".to_string())
        } else {
            Ok(format!("/{}", parts.join("/")))
        }
    }

    fn parent_and_name(path: &str) -> Option<(String, String)> {
        if path == "/" {
            return None;
        }
        let idx = path.rfind('/').expect("canonical path has a slash");
        let parent = if idx == 0 {
            "/".to_string()
        } else {
            path[..idx].to_string()
        };
        Some((parent, path[idx + 1..].to_string()))
    }

    fn components(path: &str) -> impl Iterator<Item = &str> {
        path.split('/').filter(|s| !s.is_empty())
    }

    fn resolve_existing(&self, path: &str) -> Result<&Entry, Error> {
        if path == "/" {
            return self
                .entries
                .get("/")
                .ok_or_else(|| Error::Missing("/".to_string()));
        }

        let mut prefix = String::new();
        let comps: Vec<&str> = Self::components(path).collect();
        for (i, component) in comps.iter().enumerate() {
            prefix.push('/');
            prefix.push_str(component);
            match self.entries.get(&prefix) {
                None => return Err(Error::Missing(prefix)),
                Some(entry) if i + 1 < comps.len() && !matches!(entry, Entry::Dir) => {
                    return Err(Error::NotDir(prefix));
                }
                Some(entry) if i + 1 == comps.len() => return Ok(entry),
                Some(_) => {}
            }
        }

        self.entries
            .get("/")
            .ok_or_else(|| Error::Missing("/".to_string()))
    }

    fn check_parent(&self, path: &str) -> Result<(), Error> {
        let (parent, _) = Self::parent_and_name(path).expect("non-root path");
        match self.resolve_existing(&parent)? {
            Entry::Dir => Ok(()),
            Entry::File(_) => Err(Error::NotDir(parent)),
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
        self.entries.keys().any(|candidate| {
            candidate != path && Self::is_within(candidate, path)
        })
    }

    pub fn mkdir(&mut self, path: &str) -> Result<(), Error> {
        let path = self.normalize(path)?;
        if path == "/" {
            return Err(Error::Exists(path));
        }
        self.check_parent(&path)?;
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
        self.check_parent(&path)?;
        match self.entries.get(&path) {
            Some(Entry::Dir) => Err(Error::IsDir(path)),
            Some(Entry::File(_)) | None => {
                self.entries
                    .insert(path, Entry::File(data.to_vec()));
                Ok(())
            }
        }
    }

    pub fn read(&self, path: &str) -> Result<Vec<u8>, Error> {
        let path = self.normalize(path)?;
        match self.resolve_existing(&path)? {
            Entry::File(data) => Ok(data.clone()),
            Entry::Dir => Err(Error::IsDir(path)),
        }
    }

    pub fn ls(&self, path: &str) -> Result<Vec<(String, Kind)>, Error> {
        let path = self.normalize(path)?;
        match self.resolve_existing(&path)? {
            Entry::File(_) => return Err(Error::NotDir(path)),
            Entry::Dir => {}
        }

        let mut children = Vec::new();
        for (candidate, entry) in &self.entries {
            let Some((parent, name)) = Self::parent_and_name(candidate) else {
                continue;
            };
            if parent == path {
                let kind = match entry {
                    Entry::Dir => Kind::Dir,
                    Entry::File(_) => Kind::File,
                };
                children.push((name, kind));
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
        match self.resolve_existing(&path)? {
            Entry::Dir => {
                self.cwd = path;
                Ok(())
            }
            Entry::File(_) => Err(Error::NotDir(path)),
        }
    }

    pub fn rename(&mut self, from: &str, to: &str, overwrite: bool) -> Result<(), Error> {
        let from = self.normalize(from)?;
        let to = self.normalize(to)?;

        if from == "/" || to == "/" {
            return Err(Error::Root);
        }

        self.check_parent(&from)?;
        if !self.entries.contains_key(&from) {
            return Err(Error::Missing(from));
        }

        if matches!(self.entries.get(&from), Some(Entry::Dir))
            && Self::is_within(&self.cwd, &from)
        {
            return Err(Error::Busy(from));
        }

        self.check_parent(&to)?;

        if matches!(self.entries.get(&to), Some(Entry::Dir))
            && Self::is_within(&self.cwd, &to)
        {
            return Err(Error::Busy(to));
        }

        if from == to {
            return Ok(());
        }

        let source_is_dir = matches!(self.entries.get(&from), Some(Entry::Dir));
        if source_is_dir && Self::is_within(&to, &from) {
            return Err(Error::Cycle(to));
        }

        let destination = self.entries.get(&to);
        if destination.is_some() && !overwrite {
            return Err(Error::Exists(to));
        }

        if let Some(destination_entry) = destination {
            match (source_is_dir, destination_entry) {
                (true, Entry::File(_)) => return Err(Error::NotDir(to)),
                (false, Entry::Dir) => return Err(Error::IsDir(to)),
                (true, Entry::Dir) if self.has_children(&to) => {
                    return Err(Error::NotEmpty(to));
                }
                _ => {}
            }
        }

        let moving: Vec<(String, Entry)> = self
            .entries
            .iter()
            .filter(|(path, _)| **path == from || Self::is_within(path, &from))
            .map(|(path, entry)| (path.clone(), entry.clone()))
            .collect();

        if destination.is_some() {
            self.entries.remove(&to);
        }
        for (old_path, _) in &moving {
            self.entries.remove(old_path);
        }
        for (old_path, entry) in moving {
            let suffix = &old_path[from.len()..];
            let new_path = format!("{to}{suffix}");
            self.entries.insert(new_path, entry);
        }

        Ok(())
    }

    pub fn remove(&mut self, path: &str, recursive: bool) -> Result<(), Error> {
        let path = self.normalize(path)?;
        if path == "/" {
            return Err(Error::Root);
        }

        self.check_parent(&path)?;
        if !self.entries.contains_key(&path) {
            return Err(Error::Missing(path));
        }

        if matches!(self.entries.get(&path), Some(Entry::Dir))
            && Self::is_within(&self.cwd, &path)
        {
            return Err(Error::Busy(path));
        }

        if matches!(self.entries.get(&path), Some(Entry::Dir))
            && self.has_children(&path)
            && !recursive
        {
            return Err(Error::NotEmpty(path));
        }

        let to_remove: Vec<String> = self
            .entries
            .keys()
            .filter(|candidate| **candidate == path || Self::is_within(candidate, &path))
            .cloned()
            .collect();
        for candidate in to_remove {
            self.entries.remove(&candidate);
        }
        Ok(())
    }
}

impl Default for Fs {
    fn default() -> Self {
        Self::new()
    }
}