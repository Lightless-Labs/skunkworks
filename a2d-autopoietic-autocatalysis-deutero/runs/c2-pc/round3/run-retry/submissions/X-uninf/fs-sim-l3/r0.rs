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
        entries.insert("/".to_owned(), Entry::Dir);
        Self {
            entries,
            cwd: "/".to_owned(),
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
        let mut parts: Vec<&str> = Vec::new();
        if !absolute {
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
            Ok("/".to_owned())
        } else {
            Ok(format!("/{}", parts.join("/")))
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
        self.resolve_dir(parent)
    }

    fn resolve_dir(&self, path: &str) -> Result<(), Error> {
        if path == "/" {
            return Ok(());
        }

        let mut prefix = String::new();
        let components: Vec<&str> = path.split('/').filter(|s| !s.is_empty()).collect();
        for (i, component) in components.iter().enumerate() {
            prefix.push('/');
            prefix.push_str(component);
            match self.entries.get(&prefix) {
                None => return Err(Error::Missing(prefix)),
                Some(Entry::Dir) => {}
                Some(Entry::File(_)) if i + 1 < components.len() => {
                    return Err(Error::NotDir(prefix));
                }
                Some(Entry::File(_)) => return Err(Error::NotDir(prefix)),
            }
        }
        Ok(())
    }

    fn entry(&self, path: &str) -> Result<&Entry, Error> {
        if path == "/" {
            return self.entries.get("/").ok_or_else(|| Error::Missing("/".to_owned()));
        }
        self.resolve_parent(path)?;
        self.entries
            .get(path)
            .ok_or_else(|| Error::Missing(path.to_owned()))
    }

    fn contains_path(directory: &str, path: &str) -> bool {
        if directory == "/" {
            return path.starts_with('/');
        }
        path == directory
            || path
                .strip_prefix(directory)
                .map_or(false, |rest| rest.starts_with('/'))
    }

    fn has_children(&self, path: &str) -> bool {
        self.entries
            .keys()
            .any(|candidate| candidate != path && Self::contains_path(path, candidate))
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
            Some(Entry::Dir) => Err(Error::IsDir(path)),
            Some(Entry::File(_)) | None => {
                self.entries.insert(path, Entry::File(data.to_vec()));
                Ok(())
            }
        }
    }

    pub fn read(&self, path: &str) -> Result<Vec<u8>, Error> {
        let path = self.validate_and_normalize(path)?;
        match self.entry(&path)? {
            Entry::Dir => Err(Error::IsDir(path)),
            Entry::File(data) => Ok(data.clone()),
        }
    }

    pub fn ls(&self, path: &str) -> Result<Vec<(String, Kind)>, Error> {
        let path = self.validate_and_normalize(path)?;
        match self.entry(&path)? {
            Entry::File(_) => return Err(Error::NotDir(path)),
            Entry::Dir => {}
        }

        let prefix = if path == "/" {
            "/".to_owned()
        } else {
            format!("{}/", path)
        };
        let mut children = Vec::new();
        for (candidate, entry) in &self.entries {
            if let Some(rest) = candidate.strip_prefix(&prefix) {
                if !rest.is_empty() && !rest.contains('/') {
                    children.push((rest.to_owned(), entry.kind()));
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
        let path = self.validate_and_normalize(path)?;
        match self.entry(&path)? {
            Entry::Dir => {
                self.cwd = path;
                Ok(())
            }
            Entry::File(_) => Err(Error::NotDir(path)),
        }
    }

    pub fn rename(&mut self, from: &str, to: &str, overwrite: bool) -> Result<(), Error> {
        let from = self.validate_and_normalize(from)?;
        let to = self.validate_and_normalize(to)?;

        if from == "/" || to == "/" {
            return Err(Error::Root);
        }

        self.resolve_parent(&from)?;
        let source = self
            .entries
            .get(&from)
            .cloned()
            .ok_or_else(|| Error::Missing(from.clone()))?;

        if matches!(source, Entry::Dir) && Self::contains_path(&from, &self.cwd) {
            return Err(Error::Busy(from));
        }

        self.resolve_parent(&to)?;

        if matches!(self.entries.get(&to), Some(Entry::Dir))
            && Self::contains_path(&to, &self.cwd)
        {
            return Err(Error::Busy(to));
        }

        if from == to {
            return Ok(());
        }

        if matches!(source, Entry::Dir) && Self::contains_path(&from, &to) {
            return Err(Error::Cycle(to));
        }

        let destination = self.entries.get(&to).cloned();
        if destination.is_some() && !overwrite {
            return Err(Error::Exists(to));
        }

        if let Some(dest) = &destination {
            match (&source, dest) {
                (Entry::Dir, Entry::File(_)) => return Err(Error::NotDir(to)),
                (Entry::File(_), Entry::Dir) => return Err(Error::IsDir(to)),
                (Entry::Dir, Entry::Dir) if self.has_children(&to) => {
                    return Err(Error::NotEmpty(to));
                }
                _ => {}
            }
        }

        let moving: Vec<(String, Entry)> = self
            .entries
            .iter()
            .filter(|(candidate, _)| {
                **candidate == from || Self::contains_path(&from, candidate)
            })
            .map(|(candidate, entry)| (candidate.clone(), entry.clone()))
            .collect();

        if destination.is_some() {
            self.entries.remove(&to);
        }
        for (old, _) in &moving {
            self.entries.remove(old);
        }
        for (old, entry) in moving {
            let suffix = &old[from.len()..];
            let new_path = format!("{}{}", to, suffix);
            self.entries.insert(new_path, entry);
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
            .ok_or_else(|| Error::Missing(path.clone()))?;

        if matches!(entry, Entry::Dir) && Self::contains_path(&path, &self.cwd) {
            return Err(Error::Busy(path));
        }

        if matches!(entry, Entry::Dir) && self.has_children(&path) && !recursive {
            return Err(Error::NotEmpty(path));
        }

        let descendants: Vec<String> = self
            .entries
            .keys()
            .filter(|candidate| {
                **candidate == path || Self::contains_path(&path, candidate)
            })
            .cloned()
            .collect();
        for candidate in descendants {
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