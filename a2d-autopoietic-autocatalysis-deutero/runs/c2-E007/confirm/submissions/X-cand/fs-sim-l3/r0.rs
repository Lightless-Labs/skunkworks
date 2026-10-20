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

    fn normalize(&self, path: &str) -> Result<String, Error> {
        if let Some(index) = path.as_bytes().iter().position(|&byte| byte == 0) {
            return Err(Error::InvalidPath(index));
        }
        if path.is_empty() {
            return Err(Error::InvalidPath(0));
        }

        let mut components: Vec<&str> = Vec::new();
        if !path.starts_with('/') {
            components.extend(self.cwd.split('/').filter(|part| !part.is_empty()));
        }

        for part in path.split('/') {
            match part {
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

    fn components(path: &str) -> impl Iterator<Item = &str> {
        path.split('/').filter(|part| !part.is_empty())
    }

    fn parent_and_name(path: &str) -> Option<(String, String)> {
        if path == "/" {
            return None;
        }
        let index = path.rfind('/').unwrap_or(0);
        let parent = if index == 0 {
            "/".to_string()
        } else {
            path[..index].to_string()
        };
        Some((parent, path[index + 1..].to_string()))
    }

    fn resolve_dir(&self, path: &str) -> Result<(), Error> {
        let mut prefix = String::new();
        for component in Self::components(path) {
            prefix.push('/');
            prefix.push_str(component);
            match self.entries.get(&prefix) {
                None => return Err(Error::Missing(prefix)),
                Some(Entry::Dir) => {}
                Some(Entry::File(_)) => return Err(Error::NotDir(prefix)),
            }
        }
        Ok(())
    }

    fn resolve_parent(&self, path: &str) -> Result<(String, String), Error> {
        let (parent, name) = Self::parent_and_name(path).expect("non-root path");
        self.resolve_dir(&parent)?;
        Ok((parent, name))
    }

    fn contains_path(parent: &str, candidate: &str) -> bool {
        if parent == "/" {
            candidate.starts_with('/')
        } else {
            candidate == parent
                || candidate
                    .strip_prefix(parent)
                    .is_some_and(|suffix| suffix.starts_with('/'))
        }
    }

    fn subtree_paths(&self, path: &str) -> Vec<String> {
        self.entries
            .keys()
            .filter(|candidate| Self::contains_path(path, candidate))
            .cloned()
            .collect()
    }

    pub fn mkdir(&mut self, path: &str) -> Result<(), Error> {
        let path = self.normalize(path)?;
        if path == "/" {
            return Err(Error::Exists(path));
        }
        let (parent, _) = self.resolve_parent(&path)?;
        if self.entries.contains_key(&path) {
            return Err(Error::Exists(path));
        }
        if !matches!(self.entries.get(&parent), Some(Entry::Dir)) {
            return Err(Error::NotDir(parent));
        }
        self.entries.insert(path, Entry::Dir);
        Ok(())
    }

    pub fn write(&mut self, path: &str, data: &[u8]) -> Result<(), Error> {
        let path = self.normalize(path)?;
        if path == "/" {
            return Err(Error::IsDir(path));
        }
        let (parent, _) = self.resolve_parent(&path)?;
        if let Some(entry) = self.entries.get(&path) {
            return match entry {
                Entry::Dir => Err(Error::IsDir(path)),
                Entry::File(_) => {
                    self.entries.insert(path, Entry::File(data.to_vec()));
                    Ok(())
                }
            };
        }
        if !matches!(self.entries.get(&parent), Some(Entry::Dir)) {
            return Err(Error::NotDir(parent));
        }
        self.entries.insert(path, Entry::File(data.to_vec()));
        Ok(())
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
            Some(Entry::File(data)) => Ok(data.clone()),
        }
    }

    pub fn ls(&self, path: &str) -> Result<Vec<(String, Kind)>, Error> {
        let path = self.normalize(path)?;
        self.resolve_dir(&path)?;
        if !matches!(self.entries.get(&path), Some(Entry::Dir)) {
            return Err(Error::NotDir(path));
        }

        let mut children = Vec::new();
        for (candidate, entry) in &self.entries {
            if candidate == &path {
                continue;
            }
            if let Some((parent, name)) = Self::parent_and_name(candidate) {
                if parent == path {
                    children.push((name, entry.kind()));
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
        let path = self.normalize(path)?;
        self.resolve_dir(&path)?;
        if !matches!(self.entries.get(&path), Some(Entry::Dir)) {
            return Err(Error::NotDir(path));
        }
        self.cwd = path;
        Ok(())
    }

    pub fn rename(&mut self, from: &str, to: &str, overwrite: bool) -> Result<(), Error> {
        let source = self.normalize(from)?;
        let destination = self.normalize(to)?;
        if source == "/" || destination == "/" {
            return Err(Error::Root);
        }

        let (source_parent, _) = Self::parent_and_name(&source).expect("non-root source");
        self.resolve_dir(&source_parent)?;
        let source_entry = self
            .entries
            .get(&source)
            .cloned()
            .ok_or_else(|| Error::Missing(source.clone()))?;

        if matches!(source_entry, Entry::Dir) && Self::contains_path(&source, &self.cwd) {
            return Err(Error::Busy(source));
        }

        let (destination_parent, _) =
            Self::parent_and_name(&destination).expect("non-root destination");
        self.resolve_dir(&destination_parent)?;

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

        if let Some(entry) = &destination_entry {
            match (&source_entry, entry) {
                (Entry::Dir, Entry::File(_)) => return Err(Error::NotDir(destination)),
                (Entry::File(_), Entry::Dir) => return Err(Error::IsDir(destination)),
                (Entry::Dir, Entry::Dir) => {
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

        let source_paths = self.subtree_paths(&source);
        let destination_paths = if destination_entry.is_some() {
            self.subtree_paths(&destination)
        } else {
            Vec::new()
        };

        for path in destination_paths {
            self.entries.remove(&path);
        }

        let mut moved = Vec::with_capacity(source_paths.len());
        for old_path in &source_paths {
            let suffix = &old_path[source.len()..];
            let new_path = format!("{destination}{suffix}");
            let entry = self.entries.remove(old_path).expect("source path exists");
            moved.push((new_path, entry));
        }
        for (path, entry) in moved {
            self.entries.insert(path, entry);
        }

        Ok(())
    }

    pub fn remove(&mut self, path: &str, recursive: bool) -> Result<(), Error> {
        let path = self.normalize(path)?;
        if path == "/" {
            return Err(Error::Root);
        }

        let (parent, _) = Self::parent_and_name(&path).expect("non-root path");
        self.resolve_dir(&parent)?;
        let entry = self
            .entries
            .get(&path)
            .cloned()
            .ok_or_else(|| Error::Missing(path.clone()))?;

        if matches!(entry, Entry::Dir) && Self::contains_path(&path, &self.cwd) {
            return Err(Error::Busy(path));
        }

        let subtree = self.subtree_paths(&path);
        if matches!(entry, Entry::Dir) && !recursive && subtree.len() > 1 {
            return Err(Error::NotEmpty(path));
        }

        for candidate in subtree {
            self.entries.remove(&candidate);
        }
        Ok(())
    }
}