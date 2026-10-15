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

        let mut parts: Vec<&str> = Vec::new();
        if !path.starts_with('/') {
            parts.extend(self.cwd.split('/').filter(|s| !s.is_empty()));
        }
        for part in path.split('/') {
            match part {
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

    fn parent(path: &str) -> (&str, &str) {
        if path == "/" {
            return ("/", "");
        }
        match path.rfind('/') {
            Some(0) => ("/", &path[1..]),
            Some(i) => (&path[..i], &path[i + 1..]),
            None => ("/", path),
        }
    }

    fn prefixes(path: &str) -> Vec<String> {
        if path == "/" {
            return vec!["/".to_string()];
        }
        let mut out = vec!["/".to_string()];
        let mut current = String::new();
        for component in path.split('/').filter(|s| !s.is_empty()) {
            current.push('/');
            current.push_str(component);
            out.push(current.clone());
        }
        out
    }

    fn lookup(&self, path: &str) -> Result<&Entry, Error> {
        let prefixes = Self::prefixes(path);
        for (i, prefix) in prefixes.iter().enumerate() {
            let entry = self
                .entries
                .get(prefix)
                .ok_or_else(|| Error::Missing(prefix.clone()))?;
            if i + 1 < prefixes.len() && !matches!(entry, Entry::Dir) {
                return Err(Error::NotDir(prefix.clone()));
            }
        }
        self.entries
            .get(path)
            .ok_or_else(|| Error::Missing(path.to_string()))
    }

    fn check_parent(&self, path: &str) -> Result<(), Error> {
        let (parent, _) = Self::parent(path);
        match self.lookup(parent)? {
            Entry::Dir => Ok(()),
            _ => Err(Error::NotDir(parent.to_string())),
        }
    }

    fn contains_path(dir: &str, path: &str) -> bool {
        dir == "/"
            || path == dir
            || path
                .strip_prefix(dir)
                .is_some_and(|suffix| suffix.starts_with('/'))
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
        self.check_parent(&path)?;
        if self.entries.contains_key(&path) {
            return Err(Error::Exists(path));
        }
        self.entries.insert(path, Entry::Dir);
        Ok(())
    }

    pub fn write(&mut self, path: &str, data: &[u8]) -> Result<(), Error> {
        let path = self.normalize(path)?;
        self.check_parent(&path)?;
        match self.entries.get_mut(&path) {
            Some(Entry::Dir) => Err(Error::IsDir(path)),
            Some(Entry::File(existing)) => {
                existing.clear();
                existing.extend_from_slice(data);
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
            Entry::File(data) => Ok(data.clone()),
            Entry::Dir => Err(Error::IsDir(path)),
        }
    }

    pub fn ls(&self, path: &str) -> Result<Vec<(String, Kind)>, Error> {
        let path = self.normalize(path)?;
        match self.lookup(&path)? {
            Entry::File(_) => return Err(Error::NotDir(path)),
            Entry::Dir => {}
        }

        let mut children = Vec::new();
        for (entry_path, entry) in &self.entries {
            if entry_path == "/" {
                continue;
            }
            let (parent, basename) = Self::parent(entry_path);
            if parent == path {
                children.push((basename.to_string(), entry.kind()));
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

    pub fn rename(
        &mut self,
        from: &str,
        to: &str,
        overwrite: bool,
    ) -> Result<(), Error> {
        let from = self.normalize(from)?;
        let to = self.normalize(to)?;

        if from == "/" || to == "/" {
            return Err(Error::Root);
        }

        self.check_parent(&from)?;
        let source = self
            .entries
            .get(&from)
            .ok_or_else(|| Error::Missing(from.clone()))?;

        if matches!(source, Entry::Dir) && Self::contains_path(&from, &self.cwd) {
            return Err(Error::Busy(from));
        }

        self.check_parent(&to)?;

        let destination_is_busy_dir = matches!(self.entries.get(&to), Some(Entry::Dir))
            && Self::contains_path(&to, &self.cwd);
        if destination_is_busy_dir {
            return Err(Error::Busy(to));
        }

        if from == to {
            return Ok(());
        }

        if matches!(source, Entry::Dir) && Self::contains_path(&from, &to) {
            return Err(Error::Cycle(to));
        }

        let source_kind = source.kind();
        if let Some(destination) = self.entries.get(&to) {
            if !overwrite {
                return Err(Error::Exists(to));
            }
            match (source_kind, destination.kind()) {
                (Kind::Dir, Kind::File) => return Err(Error::NotDir(to)),
                (Kind::File, Kind::Dir) => return Err(Error::IsDir(to)),
                (Kind::Dir, Kind::Dir) => {
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
        let mut moved = Vec::with_capacity(source_paths.len());
        for old_path in &source_paths {
            let suffix = &old_path[from.len()..];
            let new_path = format!("{to}{suffix}");
            let entry = self.entries.get(old_path).expect("source subtree entry").clone();
            moved.push((old_path.clone(), new_path, entry));
        }

        if self.entries.contains_key(&to) {
            for path in self.subtree_paths(&to) {
                self.entries.remove(&path);
            }
        }
        for (old_path, _, _) in &moved {
            self.entries.remove(old_path);
        }
        for (_, new_path, entry) in moved {
            self.entries.insert(new_path, entry);
        }
        Ok(())
    }

    pub fn remove(&mut self, path: &str, recursive: bool) -> Result<(), Error> {
        let path = self.normalize(path)?;
        if path == "/" {
            return Err(Error::Root);
        }

        self.check_parent(&path)?;
        let entry = self
            .entries
            .get(&path)
            .ok_or_else(|| Error::Missing(path.clone()))?;

        if matches!(entry, Entry::Dir) && Self::contains_path(&path, &self.cwd) {
            return Err(Error::Busy(path));
        }

        let paths = self.subtree_paths(&path);
        if matches!(entry, Entry::Dir) && !recursive && paths.len() > 1 {
            return Err(Error::NotEmpty(path));
        }
        for item in paths {
            self.entries.remove(&item);
        }
        Ok(())
    }
}

impl Default for Fs {
    fn default() -> Self {
        Self::new()
    }
}