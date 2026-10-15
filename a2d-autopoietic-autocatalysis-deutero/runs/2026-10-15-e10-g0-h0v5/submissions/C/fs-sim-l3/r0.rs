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

#[derive(Clone)]
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
                other => parts.push(other),
            }
        }

        if parts.is_empty() {
            Ok("/".to_string())
        } else {
            Ok(format!("/{}", parts.join("/")))
        }
    }

    fn parent_path(path: &str) -> (&str, &str) {
        if path == "/" {
            return ("/", "");
        }
        match path.rfind('/') {
            Some(0) => ("/", &path[1..]),
            Some(i) => (&path[..i], &path[i + 1..]),
            None => ("/", path),
        }
    }

    fn resolve_parent(&self, path: &str) -> Result<(), Error> {
        let (parent, _) = Self::parent_path(path);
        self.resolve_directory(parent)
    }

    fn resolve_directory(&self, path: &str) -> Result<(), Error> {
        if path == "/" {
            return Ok(());
        }

        let mut prefix = String::new();
        for component in path[1..].split('/') {
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

    fn resolve_entry(&self, path: &str) -> Result<&Entry, Error> {
        self.resolve_parent(path)?;
        self.entries
            .get(path)
            .ok_or_else(|| Error::Missing(path.to_string()))
    }

    fn contains_path(parent: &str, path: &str) -> bool {
        if parent == "/" {
            return true;
        }
        path == parent
            || (path.starts_with(parent)
                && path.as_bytes().get(parent.len()) == Some(&b'/'))
    }

    fn subtree_paths(&self, root: &str) -> Vec<String> {
        self.entries
            .keys()
            .filter(|p| Self::contains_path(root, p))
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
        let path = self.normalize(path)?;
        match self.resolve_entry(&path)? {
            Entry::Dir => Err(Error::IsDir(path)),
            Entry::File(data) => Ok(data.clone()),
        }
    }

    pub fn ls(&self, path: &str) -> Result<Vec<(String, Kind)>, Error> {
        let path = self.normalize(path)?;
        match self.resolve_entry(&path)? {
            Entry::File(_) => return Err(Error::NotDir(path)),
            Entry::Dir => {}
        }

        let mut children = Vec::new();
        for (entry_path, entry) in &self.entries {
            if entry_path == "/" || entry_path == &path {
                continue;
            }
            let (parent, basename) = Self::parent_path(entry_path);
            if parent == path {
                let kind = match entry {
                    Entry::Dir => Kind::Dir,
                    Entry::File(_) => Kind::File,
                };
                children.push((basename.to_string(), kind));
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
        match self.resolve_entry(&path)? {
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

        if self.entries.contains_key(&to) && !overwrite {
            return Err(Error::Exists(to));
        }

        if let Some(destination) = self.entries.get(&to) {
            match (&source, destination) {
                (Entry::Dir, Entry::File(_)) => return Err(Error::NotDir(to)),
                (Entry::File(_), Entry::Dir) => return Err(Error::IsDir(to)),
                (Entry::Dir, Entry::Dir) => {
                    if self
                        .entries
                        .keys()
                        .any(|p| p != &to && Self::contains_path(&to, p))
                    {
                        return Err(Error::NotEmpty(to));
                    }
                }
                _ => {}
            }
        }

        let source_paths = self.subtree_paths(&from);
        let mut moved = Vec::new();
        for old_path in &source_paths {
            let suffix = &old_path[from.len()..];
            moved.push((format!("{to}{suffix}"), self.entries[old_path].clone()));
        }

        if self.entries.contains_key(&to) {
            for p in self.subtree_paths(&to) {
                self.entries.remove(&p);
            }
        }

        for p in source_paths {
            self.entries.remove(&p);
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

        let descendants = self
            .entries
            .keys()
            .any(|p| p != &path && Self::contains_path(&path, p));
        if matches!(entry, Entry::Dir) && descendants && !recursive {
            return Err(Error::NotEmpty(path));
        }

        for p in self.subtree_paths(&path) {
            self.entries.remove(&p);
        }
        Ok(())
    }
}

fn main() {}