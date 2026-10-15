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

    fn validate_and_normalize(&self, path: &str) -> Result<String, Error> {
        if let Some(i) = path.as_bytes().iter().position(|&b| b == 0) {
            return Err(Error::InvalidPath(i));
        }
        if path.is_empty() {
            return Err(Error::InvalidPath(0));
        }

        let mut components = if path.starts_with('/') {
            Vec::new()
        } else {
            Self::components(&self.cwd)
        };

        for part in path.split('/') {
            match part {
                "" | "." => {}
                ".." => {
                    components.pop();
                }
                other => components.push(other.to_string()),
            }
        }

        Ok(Self::from_components(&components))
    }

    fn components(path: &str) -> Vec<String> {
        path.split('/')
            .filter(|part| !part.is_empty())
            .map(str::to_string)
            .collect()
    }

    fn from_components(components: &[String]) -> String {
        if components.is_empty() {
            "/".to_string()
        } else {
            format!("/{}", components.join("/"))
        }
    }

    fn parent_and_name(path: &str) -> Option<(String, String)> {
        if path == "/" {
            return None;
        }
        let split = path.rfind('/').expect("canonical paths contain slash");
        let parent = if split == 0 {
            "/".to_string()
        } else {
            path[..split].to_string()
        };
        Some((parent, path[split + 1..].to_string()))
    }

    fn resolve_parent(&self, path: &str) -> Result<String, Error> {
        let (parent, _) = Self::parent_and_name(path).expect("root has no parent");
        match self.entries.get(&parent) {
            None => Err(Error::Missing(parent)),
            Some(Entry::Dir) => Ok(parent),
            Some(Entry::File(_)) => Err(Error::NotDir(parent)),
        }
    }

    fn lookup(&self, path: &str) -> Result<&Entry, Error> {
        if path == "/" {
            return self
                .entries
                .get("/")
                .ok_or_else(|| Error::Missing("/".to_string()));
        }

        let components = Self::components(path);
        let mut prefix = String::new();
        for (i, component) in components.iter().enumerate() {
            prefix.push('/');
            prefix.push_str(component);
            match self.entries.get(&prefix) {
                None => return Err(Error::Missing(prefix)),
                Some(entry) if i + 1 < components.len() && !matches!(entry, Entry::Dir) => {
                    return Err(Error::NotDir(prefix));
                }
                Some(_) => {}
            }
        }

        self.entries
            .get(path)
            .ok_or_else(|| Error::Missing(path.to_string()))
    }

    fn contains_path(parent: &str, candidate: &str) -> bool {
        if parent == "/" {
            return candidate.starts_with('/');
        }
        candidate == parent
            || candidate
                .strip_prefix(parent)
                .is_some_and(|rest| rest.starts_with('/'))
    }

    fn is_dir_containing_cwd(&self, path: &str) -> bool {
        matches!(self.entries.get(path), Some(Entry::Dir))
            && Self::contains_path(path, &self.cwd)
    }

    fn descendants(&self, path: &str) -> Vec<String> {
        self.entries
            .keys()
            .filter(|candidate| {
                *candidate != path && Self::contains_path(path, candidate)
            })
            .cloned()
            .collect()
    }

    pub fn mkdir(&mut self, path: &str) -> Result<(), Error> {
        let path = self.validate_and_normalize(path)?;
        if path == "/" {
            return Err(Error::Exists(path));
        }

        let parent = self.resolve_parent(&path)?;
        let _ = parent;
        if self.entries.contains_key(&path) {
            return Err(Error::Exists(path));
        }

        self.entries.insert(path, Entry::Dir);
        Ok(())
    }

    pub fn write(&mut self, path: &str, data: &[u8]) -> Result<(), Error> {
        let path = self.validate_and_normalize(path)?;
        if path == "/" {
            return Err(Error::IsDir(path));
        }

        self.resolve_parent(&path)?;
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
        let path = self.validate_and_normalize(path)?;
        match self.lookup(&path)? {
            Entry::Dir => Err(Error::IsDir(path)),
            Entry::File(data) => Ok(data.clone()),
        }
    }

    pub fn ls(&self, path: &str) -> Result<Vec<(String, Kind)>, Error> {
        let path = self.validate_and_normalize(path)?;
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
        for (candidate, entry) in &self.entries {
            if let Some(rest) = candidate.strip_prefix(&prefix) {
                if !rest.is_empty() && !rest.contains('/') {
                    let kind = match entry {
                        Entry::Dir => Kind::Dir,
                        Entry::File(_) => Kind::File,
                    };
                    children.push((rest.to_string(), kind));
                }
            }
        }
        children.sort_by(|a, b| a.0.cmp(&b.0));
        Ok(children)
    }

    pub fn cwd(&self) -> String {
        self.cwd.clone()
    }

    pub fn chdir(&mut self, path: &str) -> Result<(), Error> {
        let path = self.validate_and_normalize(path)?;
        match self.lookup(&path)? {
            Entry::Dir => {
                self.cwd = path;
                Ok(())
            }
            Entry::File(_) => Err(Error::NotDir(path)),
        }
    }

    pub fn rename(&mut self, from: &str, to: &str, overwrite: bool) -> Result<(), Error> {
        let from = self.validate_and_normalize(from)?;
        let to = self.validate_and_normalize(to)?;

        if from == "/" || to == "/" {
            return Err(Error::Root);
        }

        self.resolve_parent(&from)?;
        if !self.entries.contains_key(&from) {
            return Err(Error::Missing(from));
        }
        if self.is_dir_containing_cwd(&from) {
            return Err(Error::Busy(from));
        }

        self.resolve_parent(&to)?;
        if self.is_dir_containing_cwd(&to) {
            return Err(Error::Busy(to));
        }

        if from == to {
            return Ok(());
        }

        if matches!(self.entries.get(&from), Some(Entry::Dir))
            && Self::contains_path(&from, &to)
        {
            return Err(Error::Cycle(to));
        }

        let destination_exists = self.entries.contains_key(&to);
        if destination_exists && !overwrite {
            return Err(Error::Exists(to));
        }

        if destination_exists && overwrite {
            let source_is_dir = matches!(self.entries.get(&from), Some(Entry::Dir));
            let destination_is_dir = matches!(self.entries.get(&to), Some(Entry::Dir));
            match (source_is_dir, destination_is_dir) {
                (true, false) => return Err(Error::NotDir(to)),
                (false, true) => return Err(Error::IsDir(to)),
                (true, true) => {
                    if !self.descendants(&to).is_empty() {
                        return Err(Error::NotEmpty(to));
                    }
                }
                (false, false) => {}
            }
        }

        let mut moved = Vec::new();
        for (old_path, entry) in &self.entries {
            if old_path == &from || Self::contains_path(&from, old_path) {
                let suffix = &old_path[from.len()..];
                moved.push((format!("{}{}", to, suffix), entry.clone()));
            }
        }

        let old_paths: Vec<String> = self
            .entries
            .keys()
            .filter(|candidate| {
                *candidate == &from || Self::contains_path(&from, candidate)
            })
            .cloned()
            .collect();
        for old_path in old_paths {
            self.entries.remove(&old_path);
        }

        if destination_exists {
            let destination_paths: Vec<String> = self
                .entries
                .keys()
                .filter(|candidate| {
                    *candidate == &to || Self::contains_path(&to, candidate)
                })
                .cloned()
                .collect();
            for destination_path in destination_paths {
                self.entries.remove(&destination_path);
            }
        }

        for (new_path, entry) in moved {
            self.entries.insert(new_path, entry);
        }
        Ok(())
    }

    pub fn remove(&mut self, path: &str, recursive: bool) -> Result<(), Error> {
        let path = self.validate_and_normalize(path)?;
        if path == "/" {
            return Err(Error::Root);
        }

        self.resolve_parent(&path)?;
        if !self.entries.contains_key(&path) {
            return Err(Error::Missing(path));
        }
        if self.is_dir_containing_cwd(&path) {
            return Err(Error::Busy(path));
        }

        let descendants = self.descendants(&path);
        if matches!(self.entries.get(&path), Some(Entry::Dir))
            && !descendants.is_empty()
            && !recursive
        {
            return Err(Error::NotEmpty(path));
        }

        for descendant in descendants {
            self.entries.remove(&descendant);
        }
        self.entries.remove(&path);
        Ok(())
    }
}

fn main() {}