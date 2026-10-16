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

        let mut components: Vec<String> = if path.starts_with('/') {
            Vec::new()
        } else {
            Self::components(&self.cwd)
        };

        for component in path.split('/') {
            match component {
                "" | "." => {}
                ".." => {
                    components.pop();
                }
                other => components.push(other.to_string()),
            }
        }

        Ok(Self::build_path(&components))
    }

    fn components(path: &str) -> Vec<String> {
        path.split('/')
            .filter(|part| !part.is_empty())
            .map(str::to_string)
            .collect()
    }

    fn build_path(components: &[String]) -> String {
        if components.is_empty() {
            "/".to_string()
        } else {
            format!("/{}", components.join("/"))
        }
    }

    fn parent_and_name(path: &str) -> (String, String) {
        let mut components = Self::components(path);
        let name = components.pop().unwrap_or_default();
        (Self::build_path(&components), name)
    }

    fn prefix_path(components: &[String], count: usize) -> String {
        Self::build_path(&components[..count])
    }

    fn resolve_existing(&self, path: &str) -> Result<&Entry, Error> {
        let components = Self::components(path);
        if components.is_empty() {
            return self.entries.get("/").ok_or_else(|| Error::Missing("/".to_string()));
        }

        for i in 0..components.len() {
            let prefix = Self::prefix_path(&components, i + 1);
            let entry = self
                .entries
                .get(&prefix)
                .ok_or_else(|| Error::Missing(prefix.clone()))?;

            if i + 1 < components.len() && !matches!(entry, Entry::Dir) {
                return Err(Error::NotDir(prefix));
            }
        }

        self.entries
            .get(path)
            .ok_or_else(|| Error::Missing(path.to_string()))
    }

    fn resolve_parent(&self, path: &str) -> Result<String, Error> {
        let (parent, _) = Self::parent_and_name(path);
        let _ = self.resolve_existing(&parent)?;
        match self.entries.get(&parent) {
            Some(Entry::Dir) => Ok(parent),
            Some(Entry::File(_)) => Err(Error::NotDir(parent)),
            None => Err(Error::Missing(parent)),
        }
    }

    fn is_at_or_below(path: &str, directory: &str) -> bool {
        if directory == "/" {
            return path.starts_with('/');
        }
        path == directory
            || path
                .strip_prefix(directory)
                .is_some_and(|remainder| remainder.starts_with('/'))
    }

    fn has_children(&self, path: &str) -> bool {
        self.entries.keys().any(|candidate| {
            candidate != path && Self::is_at_or_below(candidate, path)
        })
    }

    pub fn mkdir(&mut self, path: &str) -> Result<(), Error> {
        let path = self.validate_and_normalize(path)?;
        if path == "/" {
            return Err(Error::Exists(path));
        }

        let parent = self.resolve_parent(&path)?;
        if self.entries.contains_key(&path) {
            return Err(Error::Exists(path));
        }

        let _ = parent;
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
            Some(Entry::File(_)) => {
                self.entries.insert(path, Entry::File(data.to_vec()));
                Ok(())
            }
            None => {
                self.entries.insert(path, Entry::File(data.to_vec()));
                Ok(())
            }
        }
    }

    pub fn read(&self, path: &str) -> Result<Vec<u8>, Error> {
        let path = self.validate_and_normalize(path)?;
        match self.resolve_existing(&path)? {
            Entry::Dir => Err(Error::IsDir(path)),
            Entry::File(data) => Ok(data.clone()),
        }
    }

    pub fn ls(&self, path: &str) -> Result<Vec<(String, Kind)>, Error> {
        let path = self.validate_and_normalize(path)?;
        match self.resolve_existing(&path)? {
            Entry::File(_) => return Err(Error::NotDir(path)),
            Entry::Dir => {}
        }

        let mut children = Vec::new();
        for (candidate, entry) in &self.entries {
            if candidate == "/" || candidate == &path {
                continue;
            }
            let (parent, name) = Self::parent_and_name(candidate);
            if parent == path {
                children.push((name, entry.kind()));
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
        match self.resolve_existing(&path)? {
            Entry::Dir => {
                self.cwd = path;
                Ok(())
            }
            Entry::File(_) => Err(Error::NotDir(path)),
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
            .ok_or_else(|| Error::Missing(source.clone()))?;

        if matches!(source_entry, Entry::Dir) && Self::is_at_or_below(&self.cwd, &source) {
            return Err(Error::Busy(source));
        }

        self.resolve_parent(&destination)?;

        if matches!(self.entries.get(&destination), Some(Entry::Dir))
            && Self::is_at_or_below(&self.cwd, &destination)
        {
            return Err(Error::Busy(destination));
        }

        if source == destination {
            return Ok(());
        }

        if matches!(source_entry, Entry::Dir) && Self::is_at_or_below(&destination, &source) {
            return Err(Error::Cycle(destination));
        }

        let destination_entry = self.entries.get(&destination);
        if destination_entry.is_some() && !overwrite {
            return Err(Error::Exists(destination));
        }

        if let Some(existing) = destination_entry {
            match (source_entry, existing) {
                (Entry::Dir, Entry::File(_)) => return Err(Error::NotDir(destination)),
                (Entry::File(_), Entry::Dir) => return Err(Error::IsDir(destination)),
                (Entry::Dir, Entry::Dir) if self.has_children(&destination) => {
                    return Err(Error::NotEmpty(destination));
                }
                _ => {}
            }
        }

        let moved: Vec<(String, Entry)> = self
            .entries
            .iter()
            .filter(|(candidate, _)| Self::is_at_or_below(candidate, &source))
            .map(|(candidate, entry)| (candidate.clone(), entry.clone()))
            .collect();

        if destination_entry.is_some() {
            self.entries.remove(&destination);
        }
        for (old_path, _) in &moved {
            self.entries.remove(old_path);
        }

        for (old_path, entry) in moved {
            let suffix = if old_path == source {
                String::new()
            } else {
                old_path[source.len()..].to_string()
            };
            let new_path = format!("{}{}", destination, suffix);
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

        if matches!(entry, Entry::Dir) && Self::is_at_or_below(&self.cwd, &path) {
            return Err(Error::Busy(path));
        }

        if matches!(entry, Entry::Dir) && self.has_children(&path) && !recursive {
            return Err(Error::NotEmpty(path));
        }

        let to_remove: Vec<String> = self
            .entries
            .keys()
            .filter(|candidate| Self::is_at_or_below(candidate, &path))
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