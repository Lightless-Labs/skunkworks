use std::collections::{BTreeMap, BTreeSet};

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

#[derive(Clone)]
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

impl Default for Fs {
    fn default() -> Self {
        Self::new()
    }
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

    fn normalize(&self, path: &str) -> Result<String, Error> {
        if let Some(i) = path.as_bytes().iter().position(|&b| b == 0) {
            return Err(Error::InvalidPath(i));
        }
        if path.is_empty() {
            return Err(Error::InvalidPath(0));
        }

        let mut parts: Vec<String> = if path.starts_with('/') || self.cwd == "/" {
            Vec::new()
        } else {
            self.cwd[1..].split('/').map(str::to_owned).collect()
        };

        for component in path.split('/') {
            match component {
                "" | "." => {}
                ".." => {
                    parts.pop();
                }
                name => parts.push(name.to_owned()),
            }
        }

        if parts.is_empty() {
            Ok("/".to_owned())
        } else {
            Ok(format!("/{}", parts.join("/")))
        }
    }

    fn parent(path: &str) -> (&str, &str) {
        match path.rfind('/') {
            Some(0) => ("/", &path[1..]),
            Some(i) => (&path[..i], &path[i + 1..]),
            None => ("/", path),
        }
    }

    fn resolve_dir(&self, path: &str) -> Result<(), Error> {
        if path == "/" {
            return Ok(());
        }

        let mut prefix = String::new();
        for component in path[1..].split('/') {
            prefix.push('/');
            prefix.push_str(component);
            match self.entries.get(&prefix) {
                None => return Err(Error::Missing(prefix)),
                Some(Entry::File(_)) => return Err(Error::NotDir(prefix)),
                Some(Entry::Dir) => {}
            }
        }
        Ok(())
    }

    fn resolve_parent(&self, path: &str) -> Result<(), Error> {
        let (parent, _) = Self::parent(path);
        self.resolve_dir(parent)
    }

    fn contains_path(directory: &str, path: &str) -> bool {
        directory == "/"
            || path == directory
            || path
                .strip_prefix(directory)
                .is_some_and(|suffix| suffix.starts_with('/'))
    }

    fn subtree_paths(&self, root: &str) -> Vec<String> {
        self.entries
            .keys()
            .filter(|path| Self::contains_path(root, path))
            .cloned()
            .collect()
    }

    fn has_children(&self, path: &str) -> bool {
        self.entries
            .keys()
            .any(|candidate| candidate != path && Self::contains_path(path, candidate))
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
        match self.entries.get(&path) {
            Some(Entry::Dir) => return Err(Error::IsDir(path)),
            Some(Entry::File(_)) | None => {}
        }
        self.entries
            .insert(path, Entry::File(data.to_vec()));
        Ok(())
    }

    pub fn read(&self, path: &str) -> Result<Vec<u8>, Error> {
        let path = self.normalize(path)?;
        self.resolve_parent(&path)?;
        match self.entries.get(&path) {
            None => Err(Error::Missing(path)),
            Some(Entry::Dir) => Err(Error::IsDir(path)),
            Some(Entry::File(data)) => Ok(data.clone()),
        }
    }

    pub fn ls(&self, path: &str) -> Result<Vec<(String, Kind)>, Error> {
        let path = self.normalize(path)?;
        self.resolve_parent(&path)?;
        match self.entries.get(&path) {
            None => return Err(Error::Missing(path)),
            Some(Entry::File(_)) => return Err(Error::NotDir(path)),
            Some(Entry::Dir) => {}
        }

        let mut children = Vec::new();
        for (candidate, entry) in &self.entries {
            if candidate == &path || !Self::contains_path(&path, candidate) {
                continue;
            }
            let remainder = if path == "/" {
                &candidate[1..]
            } else {
                &candidate[path.len() + 1..]
            };
            if !remainder.contains('/') {
                children.push((remainder.to_owned(), entry.kind()));
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
        self.resolve_parent(&path)?;
        match self.entries.get(&path) {
            None => return Err(Error::Missing(path)),
            Some(Entry::File(_)) => return Err(Error::NotDir(path)),
            Some(Entry::Dir) => {}
        }
        self.cwd = path;
        Ok(())
    }

    pub fn rename(&mut self, from: &str, to: &str, overwrite: bool) -> Result<(), Error> {
        let from = self.normalize(from)?;
        let to = self.normalize(to)?;

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

        if overwrite {
            match (&source, &destination) {
                (Entry::Dir, Some(Entry::File(_))) => return Err(Error::NotDir(to)),
                (Entry::File(_), Some(Entry::Dir)) => return Err(Error::IsDir(to)),
                (Entry::Dir, Some(Entry::Dir)) if self.has_children(&to) => {
                    return Err(Error::NotEmpty(to));
                }
                _ => {}
            }
        }

        let source_paths = self.subtree_paths(&from);
        let moved: Vec<(String, Entry)> = source_paths
            .iter()
            .map(|old_path| {
                let suffix = &old_path[from.len()..];
                (format!("{to}{suffix}"), self.entries[old_path].clone())
            })
            .collect();

        if destination.is_some() {
            for path in self.subtree_paths(&to) {
                self.entries.remove(&path);
            }
        }
        for old_path in source_paths {
            self.entries.remove(&old_path);
        }
        for (new_path, entry) in moved {
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

        if matches!(entry, Entry::Dir) && self.has_children(&path) && !recursive {
            return Err(Error::NotEmpty(path));
        }

        for candidate in self.subtree_paths(&path) {
            self.entries.remove(&candidate);
        }
        Ok(())
    }
}