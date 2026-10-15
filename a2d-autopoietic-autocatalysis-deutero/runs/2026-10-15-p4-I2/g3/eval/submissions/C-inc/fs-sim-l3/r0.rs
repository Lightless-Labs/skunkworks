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

    fn validate_normalize(&self, path: &str) -> Result<String, Error> {
        if let Some(i) = path.as_bytes().iter().position(|&b| b == 0) {
            return Err(Error::InvalidPath(i));
        }
        if path.is_empty() {
            return Err(Error::InvalidPath(0));
        }

        let absolute = path.starts_with('/');
        let mut components: Vec<&str> = if absolute || self.cwd == "/" {
            Vec::new()
        } else {
            self.cwd[1..].split('/').collect()
        };

        for component in path.split('/') {
            match component {
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
            let mut result = String::new();
            for component in components {
                result.push('/');
                result.push_str(component);
            }
            Ok(result)
        }
    }

    fn parent_and_name(path: &str) -> Option<(&str, &str)> {
        if path == "/" {
            return None;
        }
        match path.rfind('/') {
            Some(0) => Some(("/", &path[1..])),
            Some(i) => Some((&path[..i], &path[i + 1..])),
            None => None,
        }
    }

    fn resolve_dir(&self, path: &str) -> Result<(), Error> {
        let mut prefix = String::new();
        for component in path.split('/').filter(|s| !s.is_empty()) {
            prefix.push('/');
            prefix.push_str(component);
            match self.entries.get(&prefix) {
                None => return Err(Error::Missing(prefix)),
                Some(Entry::Dir) => {}
                Some(Entry::File(_)) => return Err(Error::NotDir(prefix)),
            }
        }
        if path == "/" {
            match self.entries.get("/") {
                Some(Entry::Dir) => Ok(()),
                Some(Entry::File(_)) => Err(Error::NotDir("/".to_string())),
                None => Err(Error::Missing("/".to_string())),
            }
        } else {
            Ok(())
        }
    }

    fn resolve_parent(&self, path: &str) -> Result<(), Error> {
        let (parent, _) = Self::parent_and_name(path).expect("non-root path");
        self.resolve_dir(parent)
    }

    fn is_within(path: &str, directory: &str) -> bool {
        if directory == "/" {
            return true;
        }
        path == directory
            || path
                .strip_prefix(directory)
                .is_some_and(|suffix| suffix.starts_with('/'))
    }

    fn has_children(&self, path: &str) -> bool {
        self.entries.keys().any(|candidate| {
            candidate != path && Self::is_within(candidate, path)
        })
    }

    fn subtree_paths(&self, path: &str) -> Vec<String> {
        self.entries
            .keys()
            .filter(|candidate| *candidate == path || Self::is_within(candidate, path))
            .cloned()
            .collect()
    }

    pub fn mkdir(&mut self, path: &str) -> Result<(), Error> {
        let path = self.validate_normalize(path)?;
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
        let path = self.validate_normalize(path)?;
        if path == "/" {
            return Err(Error::IsDir(path));
        }
        self.resolve_parent(&path)?;
        match self.entries.get_mut(&path) {
            Some(Entry::Dir) => Err(Error::IsDir(path)),
            Some(Entry::File(contents)) => {
                contents.clear();
                contents.extend_from_slice(data);
                Ok(())
            }
            None => {
                self.entries.insert(path, Entry::File(data.to_vec()));
                Ok(())
            }
        }
    }

    pub fn read(&self, path: &str) -> Result<Vec<u8>, Error> {
        let path = self.validate_normalize(path)?;
        if path == "/" {
            return Err(Error::IsDir(path));
        }
        self.resolve_parent(&path)?;
        match self.entries.get(&path) {
            None => Err(Error::Missing(path)),
            Some(Entry::Dir) => Err(Error::IsDir(path)),
            Some(Entry::File(contents)) => Ok(contents.clone()),
        }
    }

    pub fn ls(&self, path: &str) -> Result<Vec<(String, Kind)>, Error> {
        let path = self.validate_normalize(path)?;
        self.resolve_dir(&path)?;
        if !matches!(self.entries.get(&path), Some(Entry::Dir)) {
            return Err(Error::NotDir(path));
        }

        let prefix = if path == "/" {
            "/".to_string()
        } else {
            format!("{path}/")
        };
        let mut result = Vec::new();
        for (candidate, entry) in &self.entries {
            if candidate == &path || !candidate.starts_with(&prefix) {
                continue;
            }
            let remainder = &candidate[prefix.len()..];
            if !remainder.contains('/') {
                result.push((remainder.to_string(), entry.kind()));
            }
        }
        result.sort_by(|a, b| a.0.cmp(&b.0));
        Ok(result)
    }

    pub fn cwd(&self) -> String {
        self.cwd.clone()
    }

    pub fn chdir(&mut self, path: &str) -> Result<(), Error> {
        let path = self.validate_normalize(path)?;
        self.resolve_dir(&path)?;
        if !matches!(self.entries.get(&path), Some(Entry::Dir)) {
            return Err(Error::NotDir(path));
        }
        self.cwd = path;
        Ok(())
    }

    pub fn rename(&mut self, from: &str, to: &str, overwrite: bool) -> Result<(), Error> {
        let from = self.validate_normalize(from)?;
        let to = self.validate_normalize(to)?;

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

        if let Some(destination_entry) = destination.as_ref() {
            match (&source, destination_entry) {
                (Entry::Dir, Entry::File(_)) => return Err(Error::NotDir(to)),
                (Entry::File(_), Entry::Dir) => return Err(Error::IsDir(to)),
                (Entry::Dir, Entry::Dir) if self.has_children(&to) => {
                    return Err(Error::NotEmpty(to))
                }
                _ => {}
            }
        }

        let source_paths = self.subtree_paths(&from);
        let mut moved = Vec::with_capacity(source_paths.len());
        for old_path in &source_paths {
            let suffix = &old_path[from.len()..];
            moved.push((format!("{to}{suffix}"), self.entries[old_path].clone()));
        }

        if destination.is_some() {
            for old_path in self.subtree_paths(&to) {
                self.entries.remove(&old_path);
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
        let path = self.validate_normalize(path)?;
        if path == "/" {
            return Err(Error::Root);
        }

        self.resolve_parent(&path)?;
        let entry = self
            .entries
            .get(&path)
            .cloned()
            .ok_or_else(|| Error::Missing(path.clone()))?;

        if matches!(entry, Entry::Dir) && Self::is_within(&self.cwd, &path) {
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

fn main() {}