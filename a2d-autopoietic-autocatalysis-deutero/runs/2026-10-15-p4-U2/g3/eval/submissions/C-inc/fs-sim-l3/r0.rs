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

    fn validate_and_normalize(&self, path: &str) -> Result<String, Error> {
        if let Some(i) = path.as_bytes().iter().position(|&b| b == 0) {
            return Err(Error::InvalidPath(i));
        }
        if path.is_empty() {
            return Err(Error::InvalidPath(0));
        }

        let absolute = path.starts_with('/');
        let mut components: Vec<&str> = Vec::new();

        if !absolute && self.cwd != "/" {
            components.extend(self.cwd[1..].split('/'));
        }

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
            Ok(format!("/{}", components.join("/")))
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

    fn resolve_parent(&self, path: &str) -> Result<(), Error> {
        let (parent, _) = Self::parent_and_name(path).expect("non-root path");
        match self.resolve_existing(parent)? {
            Entry::Dir => Ok(()),
            Entry::File(_) => Err(Error::NotDir(parent.to_string())),
        }
    }

    fn resolve_existing(&self, path: &str) -> Result<&Entry, Error> {
        if path == "/" {
            return self
                .entries
                .get("/")
                .ok_or_else(|| Error::Missing("/".to_string()));
        }

        let mut prefix = String::new();
        let mut components = path[1..].split('/').peekable();
        while let Some(component) = components.next() {
            prefix.push('/');
            prefix.push_str(component);
            let entry = self
                .entries
                .get(&prefix)
                .ok_or_else(|| Error::Missing(prefix.clone()))?;
            if components.peek().is_some() && !matches!(entry, Entry::Dir) {
                return Err(Error::NotDir(prefix));
            }
        }

        self.entries
            .get(path)
            .ok_or_else(|| Error::Missing(path.to_string()))
    }

    fn contains_path(parent: &str, child: &str) -> bool {
        parent == "/"
            || child == parent
            || child
                .strip_prefix(parent)
                .is_some_and(|rest| rest.starts_with('/'))
    }

    fn has_children(&self, path: &str) -> bool {
        self.entries
            .keys()
            .any(|candidate| candidate != path && Self::contains_path(path, candidate))
    }

    pub fn mkdir(&mut self, path: &str) -> Result<(), Error> {
        let path = self.validate_and_normalize(path)?;
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
        let path = self.validate_and_normalize(path)?;
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
                self.entries.insert(path, Entry::File(data.to_vec()));
                Ok(())
            }
        }
    }

    pub fn read(&self, path: &str) -> Result<Vec<u8>, Error> {
        let path = self.validate_and_normalize(path)?;
        match self.resolve_existing(&path)? {
            Entry::Dir => Err(Error::IsDir(path)),
            Entry::File(bytes) => Ok(bytes.clone()),
        }
    }

    pub fn ls(&self, path: &str) -> Result<Vec<(String, Kind)>, Error> {
        let path = self.validate_and_normalize(path)?;
        match self.resolve_existing(&path)? {
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
                    children.push((rest.to_string(), entry.kind()));
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
        match self.resolve_existing(&path)? {
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
        let source = self
            .entries
            .get(&from)
            .ok_or_else(|| Error::Missing(from.clone()))?
            .clone();

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

        if let Some(destination) = &destination {
            match (&source, destination) {
                (Entry::Dir, Entry::File(_)) => return Err(Error::NotDir(to)),
                (Entry::File(_), Entry::Dir) => return Err(Error::IsDir(to)),
                (Entry::Dir, Entry::Dir) if self.has_children(&to) => {
                    return Err(Error::NotEmpty(to));
                }
                _ => {}
            }
        }

        let moved: Vec<(String, Entry)> = self
            .entries
            .iter()
            .filter(|(candidate, _)| {
                **candidate == from || Self::contains_path(&from, candidate)
            })
            .map(|(candidate, entry)| (candidate.clone(), entry.clone()))
            .collect();

        for (old_path, _) in &moved {
            self.entries.remove(old_path);
        }

        if destination.is_some() {
            let remove_paths: Vec<String> = self
                .entries
                .keys()
                .filter(|candidate| {
                    **candidate == to || Self::contains_path(&to, candidate)
                })
                .cloned()
                .collect();
            for candidate in remove_paths {
                self.entries.remove(&candidate);
            }
        }

        for (old_path, entry) in moved {
            let suffix = &old_path[from.len()..];
            let new_path = format!("{}{}", to, suffix);
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
        let entry = self
            .entries
            .get(&path)
            .ok_or_else(|| Error::Missing(path.clone()))?;

        if matches!(entry, Entry::Dir) && Self::contains_path(&path, &self.cwd) {
            return Err(Error::Busy(path));
        }

        if matches!(entry, Entry::Dir) && self.has_children(&path) && !recursive {
            return Err(Error::NotEmpty(path));
        }

        let remove_paths: Vec<String> = self
            .entries
            .keys()
            .filter(|candidate| {
                **candidate == path || Self::contains_path(&path, candidate)
            })
            .cloned()
            .collect();

        for candidate in remove_paths {
            self.entries.remove(&candidate);
        }
        Ok(())
    }
}

fn main() {}