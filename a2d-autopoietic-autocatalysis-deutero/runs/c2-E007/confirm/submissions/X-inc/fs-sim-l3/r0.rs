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
        if let Some(i) = path.as_bytes().iter().position(|&b| b == 0) {
            return Err(Error::InvalidPath(i));
        }
        if path.is_empty() {
            return Err(Error::InvalidPath(0));
        }

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
                other => parts.push(other.to_string()),
            }
        }

        Ok(Self::join(&parts))
    }

    fn components(path: &str) -> Vec<String> {
        path.split('/')
            .filter(|part| !part.is_empty())
            .map(str::to_string)
            .collect()
    }

    fn join(parts: &[String]) -> String {
        if parts.is_empty() {
            "/".to_string()
        } else {
            format!("/{}", parts.join("/"))
        }
    }

    fn parent_and_name(path: &str) -> Option<(String, String)> {
        let parts = Self::components(path);
        if parts.is_empty() {
            return None;
        }
        let name = parts.last()?.clone();
        Some((Self::join(&parts[..parts.len() - 1]), name))
    }

    fn lookup(&self, path: &str) -> Result<&Entry, Error> {
        if path == "/" {
            return self.entries.get("/").ok_or_else(|| Error::Missing("/".into()));
        }

        let parts = Self::components(path);
        let mut prefix = String::new();
        for (i, part) in parts.iter().enumerate() {
            prefix.push('/');
            prefix.push_str(part);
            match self.entries.get(&prefix) {
                None => return Err(Error::Missing(prefix)),
                Some(entry) if i + 1 < parts.len() && !matches!(entry, Entry::Dir) => {
                    return Err(Error::NotDir(prefix));
                }
                Some(entry) => {
                    if i + 1 == parts.len() {
                        return Ok(entry);
                    }
                }
            }
        }
        self.entries
            .get("/")
            .ok_or_else(|| Error::Missing("/".into()))
    }

    fn check_parent(&self, path: &str) -> Result<(String, String), Error> {
        let (parent, name) =
            Self::parent_and_name(path).ok_or_else(|| Error::Exists("/".into()))?;
        match self.lookup(&parent)? {
            Entry::Dir => Ok((parent, name)),
            _ => Err(Error::NotDir(parent)),
        }
    }

    fn contains_path(parent: &str, candidate: &str) -> bool {
        parent == candidate
            || (parent == "/" && candidate.starts_with('/'))
            || (candidate.starts_with(parent)
                && candidate.as_bytes().get(parent.len()) == Some(&b'/'))
    }

    fn subtree_has_children(&self, path: &str) -> bool {
        self.entries.keys().any(|p| p != path && Self::contains_path(path, p))
    }

    fn remove_subtree(&mut self, path: &str) {
        let keys: Vec<String> = self
            .entries
            .keys()
            .filter(|p| *p == path || Self::contains_path(path, p))
            .cloned()
            .collect();
        for key in keys {
            self.entries.remove(&key);
        }
    }

    pub fn mkdir(&mut self, path: &str) -> Result<(), Error> {
        let path = self.normalize(path)?;
        if path == "/" {
            return Err(Error::Exists(path));
        }
        let (parent, name) = self.check_parent(&path)?;
        let _ = name;
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
        let _ = self.check_parent(&path)?;
        match self.entries.get_mut(&path) {
            Some(Entry::Dir) => Err(Error::IsDir(path)),
            Some(Entry::File(contents)) => {
                contents.clear();
                contents.extend_from_slice(data);
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
        match self.lookup(&path)? {
            Entry::Dir => Err(Error::IsDir(path)),
            Entry::File(contents) => Ok(contents.clone()),
        }
    }

    pub fn ls(&self, path: &str) -> Result<Vec<(String, Kind)>, Error> {
        let path = self.normalize(path)?;
        match self.lookup(&path)? {
            Entry::File(_) => return Err(Error::NotDir(path)),
            Entry::Dir => {}
        }

        let prefix = if path == "/" {
            "/".to_string()
        } else {
            format!("{}/", path)
        };
        let mut children = Vec::new();
        for (entry_path, entry) in &self.entries {
            if entry_path == &path || !entry_path.starts_with(&prefix) {
                continue;
            }
            let remainder = &entry_path[prefix.len()..];
            if !remainder.is_empty() && !remainder.contains('/') {
                children.push((remainder.to_string(), entry.kind()));
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
        match self.lookup(&path)? {
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

        let (from_parent, _) = Self::parent_and_name(&from).unwrap();
        match self.lookup(&from_parent)? {
            Entry::Dir => {}
            Entry::File(_) => return Err(Error::NotDir(from_parent)),
        }

        let source = self
            .entries
            .get(&from)
            .cloned()
            .ok_or_else(|| Error::Missing(from.clone()))?;

        if matches!(source, Entry::Dir) && Self::contains_path(&from, &self.cwd) {
            return Err(Error::Busy(from));
        }

        let (to_parent, _) = Self::parent_and_name(&to).unwrap();
        match self.lookup(&to_parent)? {
            Entry::Dir => {}
            Entry::File(_) => return Err(Error::NotDir(to_parent)),
        }

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
                (Entry::Dir, Entry::Dir) if self.subtree_has_children(&to) => {
                    return Err(Error::NotEmpty(to));
                }
                _ => {}
            }
        }

        let moving: Vec<(String, Entry)> = self
            .entries
            .iter()
            .filter(|(p, _)| *p == &from || Self::contains_path(&from, p))
            .map(|(p, e)| (p.clone(), e.clone()))
            .collect();

        if destination.is_some() {
            self.remove_subtree(&to);
        }
        self.remove_subtree(&from);

        for (old_path, entry) in moving {
            let suffix = &old_path[from.len()..];
            let new_path = format!("{}{}", to, suffix);
            self.entries.insert(new_path, entry);
        }

        Ok(())
    }

    pub fn remove(&mut self, path: &str, recursive: bool) -> Result<(), Error> {
        let path = self.normalize(path)?;
        if path == "/" {
            return Err(Error::Root);
        }

        let (parent, _) = Self::parent_and_name(&path).unwrap();
        match self.lookup(&parent)? {
            Entry::Dir => {}
            Entry::File(_) => return Err(Error::NotDir(parent)),
        }

        let entry = self
            .entries
            .get(&path)
            .ok_or_else(|| Error::Missing(path.clone()))?;

        if matches!(entry, Entry::Dir) && Self::contains_path(&path, &self.cwd) {
            return Err(Error::Busy(path));
        }

        if matches!(entry, Entry::Dir) && !recursive && self.subtree_has_children(&path) {
            return Err(Error::NotEmpty(path));
        }

        self.remove_subtree(&path);
        Ok(())
    }
}

impl Default for Fs {
    fn default() -> Self {
        Self::new()
    }
}