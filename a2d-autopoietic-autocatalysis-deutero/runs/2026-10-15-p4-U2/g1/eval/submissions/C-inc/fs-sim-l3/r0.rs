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
    entries: std::collections::BTreeMap<String, Entry>,
    cwd: String,
}

impl Fs {
    pub fn new() -> Self {
        let mut entries = std::collections::BTreeMap::new();
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

    fn parent(path: &str) -> String {
        match path.rfind('/') {
            Some(0) | None => "/".to_string(),
            Some(i) => path[..i].to_string(),
        }
    }

    fn components(path: &str) -> impl Iterator<Item = &str> {
        path.split('/').filter(|s| !s.is_empty())
    }

    fn contains_path(parent: &str, child: &str) -> bool {
        parent == child
            || (parent == "/" && child.starts_with('/'))
            || (child.len() > parent.len()
                && child.starts_with(parent)
                && child.as_bytes().get(parent.len()) == Some(&b'/'))
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

    fn resolve_parent(&self, path: &str) -> Result<(), Error> {
        let parent = Self::parent(path);
        self.resolve_dir(&parent)
    }

    fn children(&self, path: &str) -> Vec<(String, Kind)> {
        let prefix = if path == "/" {
            "/".to_string()
        } else {
            format!("{}/", path)
        };
        let mut result = Vec::new();
        for (entry_path, entry) in &self.entries {
            if entry_path == path || !entry_path.starts_with(&prefix) {
                continue;
            }
            let remainder = &entry_path[prefix.len()..];
            if !remainder.is_empty() && !remainder.contains('/') {
                result.push((remainder.to_string(), entry.kind()));
            }
        }
        result.sort_by(|a, b| a.0.cmp(&b.0));
        result
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
            Some(Entry::File(_)) => {}
            None => {}
        }
        self.entries.insert(path, Entry::File(data.to_vec()));
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
            None => Err(Error::Missing(path)),
            Some(Entry::File(_)) => Err(Error::NotDir(path)),
            Some(Entry::Dir) => Ok(self.children(&path)),
        }
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
                (Entry::Dir, Some(Entry::Dir)) if !self.children(&to).is_empty() => {
                    return Err(Error::NotEmpty(to));
                }
                _ => {}
            }
        }

        let moved: Vec<(String, Entry)> = self
            .entries
            .iter()
            .filter(|(p, _)| Self::contains_path(&from, p))
            .map(|(p, e)| (p.clone(), e.clone()))
            .collect();

        if let Some(Entry::Dir) = destination {
            self.entries.remove(&to);
        }

        for (old_path, _) in &moved {
            self.entries.remove(old_path);
        }

        for (old_path, entry) in moved {
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

        self.resolve_parent(&path)?;
        let entry = self
            .entries
            .get(&path)
            .cloned()
            .ok_or_else(|| Error::Missing(path.clone()))?;

        if matches!(entry, Entry::Dir) && Self::contains_path(&path, &self.cwd) {
            return Err(Error::Busy(path));
        }

        if matches!(entry, Entry::Dir) {
            let children = self.children(&path);
            if !recursive && !children.is_empty() {
                return Err(Error::NotEmpty(path));
            }
        }

        if recursive && matches!(entry, Entry::Dir) {
            let to_remove: Vec<String> = self
                .entries
                .keys()
                .filter(|p| Self::contains_path(&path, p))
                .cloned()
                .collect();
            for p in to_remove {
                self.entries.remove(&p);
            }
        } else {
            self.entries.remove(&path);
        }
        Ok(())
    }
}

fn main() {}