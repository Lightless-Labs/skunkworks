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

pub struct Fs {
    entries: BTreeMap<String, Entry>,
    cwd: String,
}

fn canonical(components: &[String]) -> String {
    if components.is_empty() {
        "/".to_owned()
    } else {
        format!("/{}", components.join("/"))
    }
}

fn normalize(path: &str, cwd: &str) -> Result<String, Error> {
    if let Some(i) = path.as_bytes().iter().position(|&b| b == 0) {
        return Err(Error::InvalidPath(i));
    }
    if path.is_empty() {
        return Err(Error::InvalidPath(0));
    }

    let mut components: Vec<String> = if path.starts_with('/') {
        Vec::new()
    } else {
        cwd.split('/')
            .filter(|part| !part.is_empty())
            .map(str::to_owned)
            .collect()
    };

    for part in path.split('/') {
        match part {
            "" | "." => {}
            ".." => {
                components.pop();
            }
            other => components.push(other.to_owned()),
        }
    }

    Ok(canonical(&components))
}

fn components(path: &str) -> Vec<String> {
    path.split('/')
        .filter(|part| !part.is_empty())
        .map(str::to_owned)
        .collect()
}

fn parent_and_name(path: &str) -> (String, String) {
    let mut parts = components(path);
    let name = parts.pop().unwrap_or_default();
    (canonical(&parts), name)
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

    fn resolve_dir(&self, path: &str) -> Result<(), Error> {
        let mut prefix = String::new();
        for component in components(path) {
            prefix.push('/');
            prefix.push_str(&component);
            match self.entries.get(&prefix) {
                None => return Err(Error::Missing(prefix)),
                Some(Entry::Dir) => {}
                Some(Entry::File(_)) => return Err(Error::NotDir(prefix)),
            }
        }
        Ok(())
    }

    fn resolve_parent(&self, path: &str) -> Result<(String, String), Error> {
        let (parent, name) = parent_and_name(path);
        self.resolve_dir(&parent)?;
        Ok((parent, name))
    }

    fn contains_path(parent: &str, child: &str) -> bool {
        if parent == "/" {
            child.starts_with('/')
        } else {
            child == parent
                || child
                    .strip_prefix(parent)
                    .is_some_and(|rest| rest.starts_with('/'))
        }
    }

    fn descendants(&self, path: &str) -> Vec<String> {
        self.entries
            .keys()
            .filter(|key| key.as_str() != path && Self::contains_path(path, key))
            .cloned()
            .collect()
    }

    pub fn mkdir(&mut self, path: &str) -> Result<(), Error> {
        let path = normalize(path, &self.cwd)?;
        if path == "/" {
            return Err(Error::Exists(path));
        }
        let (parent, name) = self.resolve_parent(&path)?;
        let _ = parent;
        if self.entries.contains_key(&path) {
            return Err(Error::Exists(path));
        }
        let mut full = parent_and_name(&path).0;
        if full != "/" {
            full.push('/');
        }
        full.push_str(&name);
        self.entries.insert(full, Entry::Dir);
        Ok(())
    }

    pub fn write(&mut self, path: &str, data: &[u8]) -> Result<(), Error> {
        let path = normalize(path, &self.cwd)?;
        if path == "/" {
            return Err(Error::IsDir(path));
        }
        self.resolve_parent(&path)?;
        match self.entries.get_mut(&path) {
            Some(Entry::Dir) => Err(Error::IsDir(path)),
            Some(Entry::File(bytes)) => {
                bytes.clear();
                bytes.extend_from_slice(data);
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
        let path = normalize(path, &self.cwd)?;
        if path == "/" {
            return Err(Error::IsDir(path));
        }
        self.resolve_parent(&path)?;
        match self.entries.get(&path) {
            None => Err(Error::Missing(path)),
            Some(Entry::Dir) => Err(Error::IsDir(path)),
            Some(Entry::File(bytes)) => Ok(bytes.clone()),
        }
    }

    pub fn ls(&self, path: &str) -> Result<Vec<(String, Kind)>, Error> {
        let path = normalize(path, &self.cwd)?;
        self.resolve_dir(&path)?;
        let mut result = Vec::new();
        for (entry_path, entry) in &self.entries {
            if entry_path == &path {
                continue;
            }
            let (parent, name) = parent_and_name(entry_path);
            if parent == path {
                let kind = match entry {
                    Entry::Dir => Kind::Dir,
                    Entry::File(_) => Kind::File,
                };
                result.push((name, kind));
            }
        }
        result.sort_by(|a, b| a.0.cmp(&b.0));
        Ok(result)
    }

    pub fn cwd(&self) -> String {
        self.cwd.clone()
    }

    pub fn chdir(&mut self, path: &str) -> Result<(), Error> {
        let path = normalize(path, &self.cwd)?;
        match self.entries.get(&path) {
            None => {
                self.resolve_parent(&path)?;
                Err(Error::Missing(path))
            }
            Some(Entry::File(_)) => {
                self.resolve_parent(&path)?;
                Err(Error::NotDir(path))
            }
            Some(Entry::Dir) => {
                self.resolve_dir(&path)?;
                self.cwd = path;
                Ok(())
            }
        }
    }

    pub fn rename(&mut self, from: &str, to: &str, overwrite: bool) -> Result<(), Error> {
        let from = normalize(from, &self.cwd)?;
        let to = normalize(to, &self.cwd)?;

        if from == "/" || to == "/" {
            return Err(Error::Root);
        }

        let (source_parent, _) = self.resolve_parent(&from)?;
        let source = match self.entries.get(&from) {
            None => return Err(Error::Missing(from)),
            Some(entry) => entry.clone(),
        };

        if matches!(source, Entry::Dir) && Self::contains_path(&from, &self.cwd) {
            return Err(Error::Busy(from));
        }

        let (destination_parent, _) = self.resolve_parent(&to)?;
        let _ = destination_parent;

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
                (Entry::Dir, Entry::Dir) => {
                    if !self.descendants(&to).is_empty() {
                        return Err(Error::NotEmpty(to));
                    }
                }
                _ => {}
            }
        }

        let source_paths: Vec<String> = self
            .entries
            .keys()
            .filter(|key| key.as_str() == from || Self::contains_path(&from, key))
            .cloned()
            .collect();

        let mut moved = Vec::with_capacity(source_paths.len());
        for old_path in &source_paths {
            let suffix = &old_path[from.len()..];
            let new_path = format!("{to}{suffix}");
            if let Some(entry) = self.entries.get(old_path).cloned() {
                moved.push((new_path, entry));
            }
        }

        if destination.is_some() {
            let dest_paths: Vec<String> = self
                .entries
                .keys()
                .filter(|key| key.as_str() == to || Self::contains_path(&to, key))
                .cloned()
                .collect();
            for key in dest_paths {
                self.entries.remove(&key);
            }
        }

        for old_path in source_paths {
            self.entries.remove(&old_path);
        }
        for (new_path, entry) in moved {
            self.entries.insert(new_path, entry);
        }

        let _ = source_parent;
        Ok(())
    }

    pub fn remove(&mut self, path: &str, recursive: bool) -> Result<(), Error> {
        let path = normalize(path, &self.cwd)?;
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

        let descendants = self.descendants(&path);
        if matches!(entry, Entry::Dir) && !descendants.is_empty() && !recursive {
            return Err(Error::NotEmpty(path));
        }

        for child in descendants {
            self.entries.remove(&child);
        }
        self.entries.remove(&path);
        Ok(())
    }
}