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

#[derive(Debug, Clone)]
pub struct Fs {
    entries: std::collections::BTreeMap<String, Entry>,
    cwd: String,
}

impl Default for Fs {
    fn default() -> Self {
        Self::new()
    }
}

impl Fs {
    pub fn new() -> Self {
        let mut entries = std::collections::BTreeMap::new();
        entries.insert("/".to_owned(), Entry::Dir);
        Self {
            entries,
            cwd: "/".to_owned(),
        }
    }

    fn validate(path: &str) -> Result<(), Error> {
        if let Some(i) = path.as_bytes().iter().position(|&b| b == 0) {
            return Err(Error::InvalidPath(i));
        }
        if path.is_empty() {
            return Err(Error::InvalidPath(0));
        }
        Ok(())
    }

    fn components(path: &str) -> Vec<String> {
        if path == "/" {
            Vec::new()
        } else {
            path[1..].split('/').map(str::to_owned).collect()
        }
    }

    fn canonical(parts: &[String]) -> String {
        if parts.is_empty() {
            "/".to_owned()
        } else {
            format!("/{}", parts.join("/"))
        }
    }

    fn normalize(&self, path: &str) -> Result<String, Error> {
        Self::validate(path)?;
        let mut parts = if path.starts_with('/') {
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
                other => parts.push(other.to_owned()),
            }
        }
        Ok(Self::canonical(&parts))
    }

    fn parent(path: &str) -> (String, String) {
        let parts = Self::components(path);
        let name = parts.last().cloned().unwrap_or_default();
        let parent = Self::canonical(&parts[..parts.len().saturating_sub(1)]);
        (parent, name)
    }

    fn lookup_parent(&self, path: &str) -> Result<(String, String), Error> {
        let (parent, name) = Self::parent(path);
        self.require_dir(&parent)?;
        Ok((parent, name))
    }

    fn require_dir(&self, path: &str) -> Result<(), Error> {
        match self.entries.get(path) {
            None => Err(Error::Missing(path.to_owned())),
            Some(Entry::Dir) => Ok(()),
            Some(Entry::File(_)) => Err(Error::NotDir(path.to_owned())),
        }
    }

    fn is_within(path: &str, dir: &str) -> bool {
        if dir == "/" {
            return path.starts_with('/');
        }
        path == dir
            || path
                .strip_prefix(dir)
                .is_some_and(|suffix| suffix.starts_with('/'))
    }

    fn has_children(&self, path: &str) -> bool {
        self.entries
            .keys()
            .any(|candidate| candidate != path && Self::is_within(candidate, path))
    }

    pub fn mkdir(&mut self, path: &str) -> Result<(), Error> {
        let resolved = self.normalize(path)?;
        if resolved == "/" {
            return Err(Error::Exists(resolved));
        }
        self.lookup_parent(&resolved)?;
        if self.entries.contains_key(&resolved) {
            return Err(Error::Exists(resolved));
        }
        self.entries.insert(resolved, Entry::Dir);
        Ok(())
    }

    pub fn write(&mut self, path: &str, data: &[u8]) -> Result<(), Error> {
        let resolved = self.normalize(path)?;
        if resolved == "/" {
            return Err(Error::IsDir(resolved));
        }
        self.lookup_parent(&resolved)?;
        match self.entries.get(&resolved) {
            Some(Entry::Dir) => Err(Error::IsDir(resolved)),
            Some(Entry::File(_)) | None => {
                self.entries
                    .insert(resolved, Entry::File(data.to_vec()));
                Ok(())
            }
        }
    }

    pub fn read(&self, path: &str) -> Result<Vec<u8>, Error> {
        let resolved = self.normalize(path)?;
        if resolved == "/" {
            return Err(Error::IsDir(resolved));
        }
        self.lookup_parent(&resolved)?;
        match self.entries.get(&resolved) {
            None => Err(Error::Missing(resolved)),
            Some(Entry::Dir) => Err(Error::IsDir(resolved)),
            Some(Entry::File(data)) => Ok(data.clone()),
        }
    }

    pub fn ls(&self, path: &str) -> Result<Vec<(String, Kind)>, Error> {
        let resolved = self.normalize(path)?;
        self.require_dir(&resolved)?;

        let mut children = Vec::new();
        for (candidate, entry) in &self.entries {
            if candidate == "/" || candidate == &resolved {
                continue;
            }
            let (parent, name) = Self::parent(candidate);
            if parent == resolved {
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
        let resolved = self.normalize(path)?;
        self.require_dir(&resolved)?;
        self.cwd = resolved;
        Ok(())
    }

    pub fn rename(&mut self, from: &str, to: &str, overwrite: bool) -> Result<(), Error> {
        let source = self.normalize(from)?;
        let destination = self.normalize(to)?;

        if source == "/" || destination == "/" {
            return Err(Error::Root);
        }

        self.lookup_parent(&source)?;
        let source_entry = self
            .entries
            .get(&source)
            .cloned()
            .ok_or_else(|| Error::Missing(source.clone()))?;

        if matches!(source_entry, Entry::Dir) && Self::is_within(&self.cwd, &source) {
            return Err(Error::Busy(source));
        }

        self.lookup_parent(&destination)?;

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

        let destination_entry = self.entries.get(&destination).cloned();
        if destination_entry.is_some() && !overwrite {
            return Err(Error::Exists(destination));
        }

        if let Some(existing) = &destination_entry {
            match (&source_entry, existing) {
                (Entry::Dir, Entry::File(_)) => {
                    return Err(Error::NotDir(destination));
                }
                (Entry::File(_), Entry::Dir) => {
                    return Err(Error::IsDir(destination));
                }
                (Entry::Dir, Entry::Dir) if self.has_children(&destination) => {
                    return Err(Error::NotEmpty(destination));
                }
                _ => {}
            }
        }

        let moving: Vec<(String, Entry)> = self
            .entries
            .iter()
            .filter(|(candidate, _)| Self::is_within(candidate, &source))
            .map(|(candidate, entry)| (candidate.clone(), entry.clone()))
            .collect();

        if destination_entry.is_some() {
            self.entries
                .retain(|candidate, _| !Self::is_within(candidate, &destination));
        }
        self.entries
            .retain(|candidate, _| !Self::is_within(candidate, &source));

        for (old_path, entry) in moving {
            let suffix = &old_path[source.len()..];
            let new_path = format!("{destination}{suffix}");
            self.entries.insert(new_path, entry);
        }
        Ok(())
    }

    pub fn remove(&mut self, path: &str, recursive: bool) -> Result<(), Error> {
        let resolved = self.normalize(path)?;
        if resolved == "/" {
            return Err(Error::Root);
        }

        self.lookup_parent(&resolved)?;
        let entry = self
            .entries
            .get(&resolved)
            .ok_or_else(|| Error::Missing(resolved.clone()))?;

        if matches!(entry, Entry::Dir) && Self::is_within(&self.cwd, &resolved) {
            return Err(Error::Busy(resolved));
        }

        if matches!(entry, Entry::Dir) && self.has_children(&resolved) && !recursive {
            return Err(Error::NotEmpty(resolved));
        }

        self.entries
            .retain(|candidate, _| !Self::is_within(candidate, &resolved));
        Ok(())
    }
}

fn main() {}