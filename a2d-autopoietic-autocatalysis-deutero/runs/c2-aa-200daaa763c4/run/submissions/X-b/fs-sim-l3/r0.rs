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

        for part in path.split('/') {
            match part {
                "" | "." => {}
                ".." => {
                    components.pop();
                }
                other => components.push(other.to_string()),
            }
        }

        Ok(Self::join_components(&components))
    }

    fn components(path: &str) -> Vec<String> {
        path.split('/')
            .filter(|part| !part.is_empty())
            .map(str::to_string)
            .collect()
    }

    fn join_components(components: &[String]) -> String {
        if components.is_empty() {
            "/".to_string()
        } else {
            format!("/{}", components.join("/"))
        }
    }

    fn parent_and_name(path: &str) -> (String, String) {
        let components = Self::components(path);
        if components.is_empty() {
            return ("/".to_string(), String::new());
        }
        let name = components.last().cloned().unwrap_or_default();
        let parent = Self::join_components(&components[..components.len() - 1]);
        (parent, name)
    }

    fn prefix_path(components: &[String], count: usize) -> String {
        Self::join_components(&components[..count])
    }

    fn resolve_existing(&self, path: &str) -> Result<&Entry, Error> {
        if path == "/" {
            return self
                .entries
                .get("/")
                .ok_or_else(|| Error::Missing("/".to_string()));
        }

        let components = Self::components(path);
        for i in 0..components.len() {
            let prefix = Self::prefix_path(&components, i + 1);
            match self.entries.get(&prefix) {
                None => return Err(Error::Missing(prefix)),
                Some(entry) if i + 1 < components.len() && !matches!(entry, Entry::Dir) => {
                    return Err(Error::NotDir(prefix));
                }
                Some(_) => {}
            }
        }

        self.entries
            .get(path)
            .ok_or_else(|| Error::Missing(path.to_string()))
    }

    fn resolve_parent(&self, path: &str) -> Result<(String, String), Error> {
        let (parent, name) = Self::parent_and_name(path);
        match self.resolve_existing(&parent)? {
            Entry::Dir => Ok((parent, name)),
            _ => Err(Error::NotDir(parent)),
        }
    }

    fn is_at_or_below(path: &str, ancestor: &str) -> bool {
        if ancestor == "/" {
            return true;
        }
        path == ancestor
            || path
                .strip_prefix(ancestor)
                .is_some_and(|suffix| suffix.starts_with('/'))
    }

    fn direct_children(&self, path: &str) -> Vec<(String, Kind)> {
        let prefix = if path == "/" {
            "/".to_string()
        } else {
            format!("{path}/")
        };
        let mut children = Vec::new();
        for (entry_path, entry) in &self.entries {
            if entry_path == path || !entry_path.starts_with(&prefix) {
                continue;
            }
            let remainder = &entry_path[prefix.len()..];
            if !remainder.is_empty() && !remainder.contains('/') {
                children.push((remainder.to_string(), entry.kind()));
            }
        }
        children.sort_by(|a, b| a.0.cmp(&b.0));
        children
    }

    pub fn mkdir(&mut self, path: &str) -> Result<(), Error> {
        let canonical = self.validate_and_normalize(path)?;
        let (parent, name) = self.resolve_parent(&canonical)?;
        if canonical == "/" {
            return Err(Error::Exists(canonical));
        }
        let full_path = if parent == "/" {
            format!("/{name}")
        } else {
            format!("{parent}/{name}")
        };
        if self.entries.contains_key(&full_path) {
            return Err(Error::Exists(full_path));
        }
        self.entries.insert(full_path, Entry::Dir);
        Ok(())
    }

    pub fn write(&mut self, path: &str, data: &[u8]) -> Result<(), Error> {
        let canonical = self.validate_and_normalize(path)?;
        if canonical == "/" {
            return Err(Error::IsDir(canonical));
        }
        let (parent, name) = self.resolve_parent(&canonical)?;
        let full_path = if parent == "/" {
            format!("/{name}")
        } else {
            format!("{parent}/{name}")
        };
        match self.entries.get_mut(&full_path) {
            Some(Entry::Dir) => Err(Error::IsDir(full_path)),
            Some(Entry::File(bytes)) => {
                bytes.clear();
                bytes.extend_from_slice(data);
                Ok(())
            }
            None => {
                self.entries
                    .insert(full_path, Entry::File(data.to_vec()));
                Ok(())
            }
        }
    }

    pub fn read(&self, path: &str) -> Result<Vec<u8>, Error> {
        let canonical = self.validate_and_normalize(path)?;
        match self.resolve_existing(&canonical)? {
            Entry::Dir => Err(Error::IsDir(canonical)),
            Entry::File(bytes) => Ok(bytes.clone()),
        }
    }

    pub fn ls(&self, path: &str) -> Result<Vec<(String, Kind)>, Error> {
        let canonical = self.validate_and_normalize(path)?;
        match self.resolve_existing(&canonical)? {
            Entry::Dir => Ok(self.direct_children(&canonical)),
            Entry::File(_) => Err(Error::NotDir(canonical)),
        }
    }

    pub fn cwd(&self) -> String {
        self.cwd.clone()
    }

    pub fn chdir(&mut self, path: &str) -> Result<(), Error> {
        let canonical = self.validate_and_normalize(path)?;
        match self.resolve_existing(&canonical)? {
            Entry::Dir => {
                self.cwd = canonical;
                Ok(())
            }
            Entry::File(_) => Err(Error::NotDir(canonical)),
        }
    }

    pub fn rename(&mut self, from: &str, to: &str, overwrite: bool) -> Result<(), Error> {
        let source = self.validate_and_normalize(from)?;
        let destination = self.validate_and_normalize(to)?;

        if source == "/" || destination == "/" {
            return Err(Error::Root);
        }

        let (source_parent, source_name) = self.resolve_parent(&source)?;
        let _ = (source_parent, source_name);
        let source_entry = self
            .entries
            .get(&source)
            .ok_or_else(|| Error::Missing(source.clone()))?;

        if matches!(source_entry, Entry::Dir) && Self::is_at_or_below(&self.cwd, &source) {
            return Err(Error::Busy(source));
        }

        let (destination_parent, destination_name) = self.resolve_parent(&destination)?;
        let _ = (destination_parent, destination_name);

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

        if let Some(dest_entry) = destination_entry {
            match (source_entry, dest_entry) {
                (Entry::Dir, Entry::File(_)) => {
                    return Err(Error::NotDir(destination));
                }
                (Entry::File(_), Entry::Dir) => {
                    return Err(Error::IsDir(destination));
                }
                (Entry::Dir, Entry::Dir) => {
                    if !self.direct_children(&destination).is_empty() {
                        return Err(Error::NotEmpty(destination));
                    }
                }
                _ => {}
            }
        }

        let source_prefix = format!("{source}/");
        let moving: Vec<(String, Entry)> = self
            .entries
            .iter()
            .filter(|(path, _)| **path == source || path.starts_with(&source_prefix))
            .map(|(path, entry)| (path.clone(), entry.clone()))
            .collect();

        if destination_entry.is_some() {
            let destination_prefix = format!("{destination}/");
            self.entries.retain(|path, _| {
                path != &destination && !path.starts_with(&destination_prefix)
            });
        }

        self.entries.retain(|path, _| {
            path != &source && !path.starts_with(&source_prefix)
        });

        for (old_path, entry) in moving {
            let suffix = &old_path[source.len()..];
            self.entries.insert(format!("{destination}{suffix}"), entry);
        }

        Ok(())
    }

    pub fn remove(&mut self, path: &str, recursive: bool) -> Result<(), Error> {
        let canonical = self.validate_and_normalize(path)?;
        if canonical == "/" {
            return Err(Error::Root);
        }

        let (parent, _) = self.resolve_parent(&canonical)?;
        let _ = parent;
        let entry = self
            .entries
            .get(&canonical)
            .ok_or_else(|| Error::Missing(canonical.clone()))?;

        if matches!(entry, Entry::Dir) && Self::is_at_or_below(&self.cwd, &canonical) {
            return Err(Error::Busy(canonical));
        }

        if matches!(entry, Entry::Dir)
            && !recursive
            && !self.direct_children(&canonical).is_empty()
        {
            return Err(Error::NotEmpty(canonical));
        }

        let prefix = format!("{canonical}/");
        self.entries.retain(|entry_path, _| {
            entry_path != &canonical && ( !recursive || !entry_path.starts_with(&prefix))
        });
        Ok(())
    }
}

impl Default for Fs {
    fn default() -> Self {
        Self::new()
    }
}