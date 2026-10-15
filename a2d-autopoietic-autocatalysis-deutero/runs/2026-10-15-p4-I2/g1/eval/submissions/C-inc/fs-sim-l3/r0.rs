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

        let mut parts: Vec<&str> = Vec::new();
        if !path.starts_with('/') {
            parts.extend(self.cwd.split('/').filter(|part| !part.is_empty()));
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
            Ok("/".to_owned())
        } else {
            let mut result = String::new();
            for part in parts {
                result.push('/');
                result.push_str(part);
            }
            Ok(result)
        }
    }

    fn parent(path: &str) -> Option<String> {
        if path == "/" {
            return None;
        }
        match path.rfind('/') {
            Some(0) => Some("/".to_owned()),
            Some(i) => Some(path[..i].to_owned()),
            None => Some("/".to_owned()),
        }
    }

    fn components(path: &str) -> impl Iterator<Item = &str> {
        path.split('/').filter(|part| !part.is_empty())
    }

    fn is_within(path: &str, directory: &str) -> bool {
        if directory == "/" {
            return path.starts_with('/');
        }
        path == directory
            || path
                .strip_prefix(directory)
                .is_some_and(|rest| rest.starts_with('/'))
    }

    fn lookup_parent(&self, path: &str) -> Result<(), Error> {
        let mut prefix = String::from("/");
        let components: Vec<&str> = Self::components(path).collect();
        for component in components.iter().take(components.len().saturating_sub(1)) {
            if prefix != "/" {
                prefix.push('/');
            }
            prefix.push_str(component);
            match self.entries.get(&prefix) {
                None => return Err(Error::Missing(prefix)),
                Some(Entry::File(_)) => return Err(Error::NotDir(prefix)),
                Some(Entry::Dir) => {}
            }
        }
        Ok(())
    }

    fn lookup_entry(&self, path: &str) -> Result<&Entry, Error> {
        if path == "/" {
            return self
                .entries
                .get("/")
                .ok_or_else(|| Error::Missing("/".to_owned()));
        }

        let mut prefix = String::from("/");
        let components: Vec<&str> = Self::components(path).collect();
        for (index, component) in components.iter().enumerate() {
            if prefix != "/" {
                prefix.push('/');
            }
            prefix.push_str(component);

            match self.entries.get(&prefix) {
                None => return Err(Error::Missing(prefix)),
                Some(entry) if index + 1 < components.len() && matches!(entry, Entry::File(_)) => {
                    return Err(Error::NotDir(prefix));
                }
                Some(entry) => {
                    if index + 1 == components.len() {
                        return Ok(entry);
                    }
                }
            }
        }

        self.entries
            .get("/")
            .ok_or_else(|| Error::Missing("/".to_owned()))
    }

    fn has_children(&self, path: &str) -> bool {
        self.entries
            .keys()
            .any(|candidate| candidate != path && Self::is_within(candidate, path))
    }

    pub fn mkdir(&mut self, path: &str) -> Result<(), Error> {
        let path = self.normalize(path)?;
        if path == "/" {
            return Err(Error::Exists(path));
        }

        self.lookup_parent(&path)?;
        let parent = Self::parent(&path).expect("non-root path has a parent");
        match self.entries.get(&parent) {
            None => return Err(Error::Missing(parent)),
            Some(Entry::File(_)) => return Err(Error::NotDir(parent)),
            Some(Entry::Dir) => {}
        }

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

        self.lookup_parent(&path)?;
        let parent = Self::parent(&path).expect("non-root path has a parent");
        match self.entries.get(&parent) {
            None => return Err(Error::Missing(parent)),
            Some(Entry::File(_)) => return Err(Error::NotDir(parent)),
            Some(Entry::Dir) => {}
        }

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
        match self.lookup_entry(&path)? {
            Entry::Dir => Err(Error::IsDir(path)),
            Entry::File(contents) => Ok(contents.clone()),
        }
    }

    pub fn ls(&self, path: &str) -> Result<Vec<(String, Kind)>, Error> {
        let path = self.normalize(path)?;
        match self.lookup_entry(&path)? {
            Entry::File(_) => return Err(Error::NotDir(path)),
            Entry::Dir => {}
        }

        let mut children = Vec::new();
        for (candidate, entry) in &self.entries {
            if candidate == &path {
                continue;
            }
            let Some(parent) = Self::parent(candidate) else {
                continue;
            };
            if parent == path {
                let basename = candidate.rsplit('/').next().unwrap_or("").to_owned();
                children.push((basename, entry.kind()));
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
        match self.lookup_entry(&path)? {
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

        self.lookup_parent(&from)?;
        let source_parent = Self::parent(&from).expect("non-root path has a parent");
        match self.entries.get(&source_parent) {
            None => return Err(Error::Missing(source_parent)),
            Some(Entry::File(_)) => return Err(Error::NotDir(source_parent)),
            Some(Entry::Dir) => {}
        }

        let source = self
            .entries
            .get(&from)
            .ok_or_else(|| Error::Missing(from.clone()))?;

        if matches!(source, Entry::Dir) && Self::is_within(&self.cwd, &from) {
            return Err(Error::Busy(from));
        }

        self.lookup_parent(&to)?;
        let destination_parent = Self::parent(&to).expect("non-root path has a parent");
        match self.entries.get(&destination_parent) {
            None => return Err(Error::Missing(destination_parent)),
            Some(Entry::File(_)) => return Err(Error::NotDir(destination_parent)),
            Some(Entry::Dir) => {}
        }

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

        let destination_exists = self.entries.contains_key(&to);
        if destination_exists && !overwrite {
            return Err(Error::Exists(to));
        }

        if destination_exists {
            let source_kind = source.kind();
            let destination_kind = self.entries.get(&to).expect("entry exists").kind();
            match (source_kind, destination_kind) {
                (Kind::Dir, Kind::File) => return Err(Error::NotDir(to)),
                (Kind::File, Kind::Dir) => return Err(Error::IsDir(to)),
                (Kind::Dir, Kind::Dir) if self.has_children(&to) => {
                    return Err(Error::NotEmpty(to));
                }
                _ => {}
            }
        }

        let moving: Vec<(String, Entry)> = self
            .entries
            .iter()
            .filter(|(candidate, _)| Self::is_within(candidate, &from))
            .map(|(candidate, entry)| (candidate.clone(), entry.clone()))
            .collect();

        if destination_exists {
            self.entries.remove(&to);
        }
        for (old_path, _) in &moving {
            self.entries.remove(old_path);
        }
        for (old_path, entry) in moving {
            let suffix = &old_path[from.len()..];
            let new_path = format!("{to}{suffix}");
            self.entries.insert(new_path, entry);
        }

        Ok(())
    }

    pub fn remove(&mut self, path: &str, recursive: bool) -> Result<(), Error> {
        let path = self.normalize(path)?;
        if path == "/" {
            return Err(Error::Root);
        }

        self.lookup_parent(&path)?;
        let parent = Self::parent(&path).expect("non-root path has a parent");
        match self.entries.get(&parent) {
            None => return Err(Error::Missing(parent)),
            Some(Entry::File(_)) => return Err(Error::NotDir(parent)),
            Some(Entry::Dir) => {}
        }

        let entry = self
            .entries
            .get(&path)
            .ok_or_else(|| Error::Missing(path.clone()))?;

        if matches!(entry, Entry::Dir) && Self::is_within(&self.cwd, &path) {
            return Err(Error::Busy(path));
        }

        if matches!(entry, Entry::Dir) && self.has_children(&path) && !recursive {
            return Err(Error::NotEmpty(path));
        }

        let to_remove: Vec<String> = self
            .entries
            .keys()
            .filter(|candidate| {
                *candidate == &path
                    || (recursive && Self::is_within(candidate, &path))
            })
            .cloned()
            .collect();
        for candidate in to_remove {
            self.entries.remove(&candidate);
        }
        Ok(())
    }
}

impl Default for Fs {
    fn default() -> Self {
        Self::new()
    }
}

fn main() {}