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
    entries: std::collections::HashMap<String, Entry>,
    cwd: String,
}

impl Fs {
    pub fn new() -> Self {
        let mut entries = std::collections::HashMap::new();
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
            parts.extend(self.cwd.split('/').filter(|part| !part.is_empty()));
        }

        for part in path.split('/') {
            match part {
                "" | "." => {}
                ".." => {
                    parts.pop();
                }
                _ => parts.push(part),
            }
        }

        if parts.is_empty() {
            Ok("/".to_string())
        } else {
            Ok(format!("/{}", parts.join("/")))
        }
    }

    fn parent(path: &str) -> Option<String> {
        if path == "/" {
            return None;
        }
        match path.rfind('/') {
            Some(0) => Some("/".to_string()),
            Some(i) => Some(path[..i].to_string()),
            None => Some("/".to_string()),
        }
    }

    fn is_within(path: &str, directory: &str) -> bool {
        if directory == "/" {
            return path.starts_with('/');
        }
        path == directory
            || (path.starts_with(directory)
                && path.as_bytes().get(directory.len()) == Some(&b'/'))
    }

    fn resolve_existing(&self, path: &str) -> Result<&Entry, Error> {
        if path == "/" {
            return self.entries.get("/").ok_or_else(|| Error::Missing("/".into()));
        }

        let mut prefix = String::new();
        let components: Vec<&str> = path[1..].split('/').collect();
        for (i, component) in components.iter().enumerate() {
            prefix.push('/');
            prefix.push_str(component);

            match self.entries.get(&prefix) {
                None => return Err(Error::Missing(prefix)),
                Some(entry) if i + 1 < components.len() && !matches!(entry, Entry::Dir) => {
                    return Err(Error::NotDir(prefix));
                }
                Some(entry) if i + 1 == components.len() => return Ok(entry),
                Some(_) => {}
            }
        }

        self.entries
            .get(path)
            .ok_or_else(|| Error::Missing(path.to_string()))
    }

    fn resolve_parent(&self, path: &str) -> Result<String, Error> {
        let parent = Self::parent(path).expect("non-root path has a parent");
        match self.resolve_existing(&parent)? {
            Entry::Dir => Ok(parent),
            _ => Err(Error::NotDir(parent)),
        }
    }

    fn subtree_paths(&self, path: &str) -> Vec<String> {
        self.entries
            .keys()
            .filter(|candidate| Self::is_within(candidate, path))
            .cloned()
            .collect()
    }

    pub fn mkdir(&mut self, path: &str) -> Result<(), Error> {
        let path = self.normalize(path)?;
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
        let path = self.normalize(path)?;
        if path == "/" {
            return Err(Error::IsDir(path));
        }
        self.resolve_parent(&path)?;

        match self.entries.get(&path) {
            Some(Entry::Dir) => Err(Error::IsDir(path)),
            Some(Entry::File(_)) => {
                self.entries
                    .insert(path, Entry::File(data.to_vec()));
                Ok(())
            }
            None => {
                self.entries.insert(path, Entry::File(data.to_vec()));
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
            if candidate == &path || !Self::is_within(candidate, &path) {
                continue;
            }
            let Some(parent) = Self::parent(candidate) else {
                continue;
            };
            if parent == path {
                let name = candidate.rsplit('/').next().unwrap_or("").to_string();
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
        let path = self.normalize(path)?;
        match self.resolve_existing(&path)? {
            Entry::Dir => {
                self.cwd = path;
                Ok(())
            }
            _ => Err(Error::NotDir(path)),
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

        if matches!(source, Entry::Dir) && Self::is_within(&self.cwd, &from) {
            return Err(Error::Busy(from));
        }

        self.resolve_parent(&to)?;

        if let Some(Entry::Dir) = self.entries.get(&to) {
            if Self::is_within(&self.cwd, &to) {
                return Err(Error::Busy(to));
            }
        }

        if from == to {
            return Ok(());
        }

        if matches!(source, Entry::Dir) && Self::is_within(&to, &from) {
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
                    if self.entries.keys().any(|p| p != &to && Self::is_within(p, &to)) {
                        return Err(Error::NotEmpty(to));
                    }
                }
                _ => {}
            }
        }

        let source_paths = self.subtree_paths(&from);
        let moving: Vec<(String, Entry)> = source_paths
            .iter()
            .filter_map(|old| {
                self.entries.get(old).cloned().map(|entry| {
                    let suffix = &old[from.len()..];
                    (format!("{to}{suffix}"), entry)
                })
            })
            .collect();

        for old in source_paths {
            self.entries.remove(&old);
        }

        if self.entries.contains_key(&to) {
            let destination_paths = self.subtree_paths(&to);
            for old in destination_paths {
                self.entries.remove(&old);
            }
        }

        for (new_path, entry) in moving {
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
            .ok_or_else(|| Error::Missing(path.clone()))?;

        if matches!(entry, Entry::Dir) && Self::is_within(&self.cwd, &path) {
            return Err(Error::Busy(path));
        }

        let descendants: Vec<String> = self
            .entries
            .keys()
            .filter(|candidate| candidate.as_str() != path && Self::is_within(candidate, &path))
            .cloned()
            .collect();

        if matches!(entry, Entry::Dir) && !recursive && !descendants.is_empty() {
            return Err(Error::NotEmpty(path));
        }

        self.entries.remove(&path);
        if recursive {
            for descendant in descendants {
                self.entries.remove(&descendant);
            }
        }
        Ok(())
    }
}

fn main() {}