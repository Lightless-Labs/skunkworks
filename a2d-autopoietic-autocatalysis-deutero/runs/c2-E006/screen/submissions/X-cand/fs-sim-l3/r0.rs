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

#[derive(Debug, Clone, PartialEq, Eq)]
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

        let mut components: Vec<&str> = Vec::new();
        if !path.starts_with('/') {
            components.extend(self.cwd.split('/').filter(|c| !c.is_empty()));
        }

        for component in path.split('/') {
            match component {
                "" | "." => {}
                ".." => {
                    components.pop();
                }
                c => components.push(c),
            }
        }

        if components.is_empty() {
            Ok("/".to_string())
        } else {
            Ok(format!("/{}", components.join("/")))
        }
    }

    fn parent(path: &str) -> (&str, &str) {
        let index = path.rfind('/').unwrap_or(0);
        let name = &path[index + 1..];
        let parent = if index == 0 { "/" } else { &path[..index] };
        (parent, name)
    }

    fn is_within(path: &str, directory: &str) -> bool {
        if directory == "/" {
            return path.starts_with('/');
        }
        path == directory
            || (path.starts_with(directory)
                && path.as_bytes().get(directory.len()) == Some(&b'/'))
    }

    fn check_directory(&self, path: &str) -> Result<(), Error> {
        let mut current = String::new();
        for component in path.split('/').filter(|c| !c.is_empty()) {
            current.push('/');
            current.push_str(component);
            match self.entries.get(&current) {
                None => return Err(Error::Missing(current)),
                Some(Entry::Dir) => {}
                Some(Entry::File(_)) => return Err(Error::NotDir(current)),
            }
        }
        Ok(())
    }

    fn check_parent(&self, path: &str) -> Result<(), Error> {
        let (parent, _) = Self::parent(path);
        self.check_directory(parent)
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
        self.check_parent(&path)?;
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
        self.check_parent(&path)?;
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
        let path = self.normalize(path)?;
        self.check_parent(&path)?;
        match self.entries.get(&path) {
            None => Err(Error::Missing(path)),
            Some(Entry::Dir) => Err(Error::IsDir(path)),
            Some(Entry::File(data)) => Ok(data.clone()),
        }
    }

    pub fn ls(&self, path: &str) -> Result<Vec<(String, Kind)>, Error> {
        let path = self.normalize(path)?;
        self.check_directory(&path)?;
        let mut children = Vec::new();

        for (entry_path, entry) in &self.entries {
            if entry_path == "/" || entry_path == &path {
                continue;
            }
            let (parent, name) = Self::parent(entry_path);
            if parent == path {
                let kind = match entry {
                    Entry::Dir => Kind::Dir,
                    Entry::File(_) => Kind::File,
                };
                children.push((name.to_string(), kind));
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
        self.check_parent(&path)?;
        match self.entries.get(&path) {
            None => Err(Error::Missing(path)),
            Some(Entry::File(_)) => Err(Error::NotDir(path)),
            Some(Entry::Dir) => {
                self.cwd = path;
                Ok(())
            }
        }
    }

    pub fn rename(&mut self, from: &str, to: &str, overwrite: bool) -> Result<(), Error> {
        let from = self.normalize(from)?;
        let to = self.normalize(to)?;

        if from == "/" || to == "/" {
            return Err(Error::Root);
        }

        self.check_parent(&from)?;
        let source = match self.entries.get(&from) {
            None => return Err(Error::Missing(from)),
            Some(entry) => entry.clone(),
        };

        if matches!(source, Entry::Dir) && Self::is_within(&self.cwd, &from) {
            return Err(Error::Busy(from));
        }

        self.check_parent(&to)?;

        if matches!(self.entries.get(&to), Some(Entry::Dir))
            && Self::is_within(&self.cwd, &to)
        {
            return Err(Error::Busy(to));
        }

        if from == to {
            return Ok(());
        }

        if matches!(source, Entry::Dir) && Self::is_within(&to, &from) {
            return Err(Error::Cycle(to));
        }

        let destination = self.entries.get(&to).cloned();
        if destination.is_some() && !overwrite {
            return Err(Error::Exists(to));
        }

        if let Some(ref destination_entry) = destination {
            match (&source, destination_entry) {
                (Entry::Dir, Entry::File(_)) => return Err(Error::NotDir(to)),
                (Entry::File(_), Entry::Dir) => return Err(Error::IsDir(to)),
                (Entry::Dir, Entry::Dir) => {
                    if self.entries.keys().any(|candidate| {
                        candidate != &to && Self::is_within(candidate, &to)
                    }) {
                        return Err(Error::NotEmpty(to));
                    }
                }
                (Entry::File(_), Entry::File(_)) => {}
            }
        }

        let source_paths = self.subtree_paths(&from);
        let moving: Vec<(String, Entry)> = source_paths
            .iter()
            .filter_map(|old_path| {
                self.entries.get(old_path).map(|entry| {
                    let suffix = &old_path[from.len()..];
                    (format!("{to}{suffix}"), entry.clone())
                })
            })
            .collect();

        if destination.is_some() {
            for path in self.subtree_paths(&to) {
                self.entries.remove(&path);
            }
        }
        for path in source_paths {
            self.entries.remove(&path);
        }
        for (path, entry) in moving {
            self.entries.insert(path, entry);
        }

        Ok(())
    }

    pub fn remove(&mut self, path: &str, recursive: bool) -> Result<(), Error> {
        let path = self.normalize(path)?;
        if path == "/" {
            return Err(Error::Root);
        }

        self.check_parent(&path)?;
        let entry = match self.entries.get(&path) {
            None => return Err(Error::Missing(path)),
            Some(entry) => entry,
        };

        if matches!(entry, Entry::Dir) && Self::is_within(&self.cwd, &path) {
            return Err(Error::Busy(path));
        }

        if matches!(entry, Entry::Dir)
            && !recursive
            && self.entries.keys().any(|candidate| {
                candidate != &path && Self::is_within(candidate, &path)
            })
        {
            return Err(Error::NotEmpty(path));
        }

        for candidate in self.subtree_paths(&path) {
            self.entries.remove(&candidate);
        }
        Ok(())
    }
}