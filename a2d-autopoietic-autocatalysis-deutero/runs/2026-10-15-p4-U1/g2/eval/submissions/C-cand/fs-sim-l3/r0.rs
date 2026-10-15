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

        let mut parts: Vec<String> = if path.starts_with('/') {
            Vec::new()
        } else {
            Self::components(&self.cwd)
        };

        for part in path.split('/') {
            match part {
                "" | "." => {}
                ".." => {
                    parts.pop();
                }
                name => parts.push(name.to_string()),
            }
        }
        Ok(Self::from_components(&parts))
    }

    fn components(path: &str) -> Vec<String> {
        path.split('/')
            .filter(|part| !part.is_empty())
            .map(str::to_string)
            .collect()
    }

    fn from_components(parts: &[String]) -> String {
        if parts.is_empty() {
            "/".to_string()
        } else {
            format!("/{}", parts.join("/"))
        }
    }

    fn parent(path: &str) -> String {
        if path == "/" {
            return "/".to_string();
        }
        match path.rfind('/') {
            Some(0) | None => "/".to_string(),
            Some(i) => path[..i].to_string(),
        }
    }

    fn name(path: &str) -> &str {
        path.rsplit('/').next().unwrap_or("")
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

    fn resolve_existing(&self, path: &str) -> Result<&Entry, Error> {
        if path == "/" {
            return Ok(self.entries.get("/").expect("root exists"));
        }

        let parts = Self::components(path);
        let mut prefix = String::new();
        for (i, part) in parts.iter().enumerate() {
            prefix.push('/');
            prefix.push_str(part);
            match self.entries.get(&prefix) {
                None => return Err(Error::Missing(prefix)),
                Some(entry) if i + 1 < parts.len() && entry.kind() != Kind::Dir => {
                    return Err(Error::NotDir(prefix));
                }
                Some(entry) if i + 1 == parts.len() => return Ok(entry),
                Some(_) => {}
            }
        }
        Ok(self.entries.get("/").expect("root exists"))
    }

    fn resolve_parent(&self, path: &str) -> Result<String, Error> {
        let parent = Self::parent(path);
        match self.resolve_existing(&parent)? {
            Entry::Dir => Ok(parent),
            _ => Err(Error::NotDir(parent)),
        }
    }

    fn subtree_keys(&self, path: &str) -> Vec<String> {
        self.entries
            .keys()
            .filter(|candidate| Self::is_within(candidate, path))
            .cloned()
            .collect()
    }

    pub fn mkdir(&mut self, path: &str) -> Result<(), Error> {
        let canonical = self.normalize(path)?;
        self.resolve_parent(&canonical)?;
        if self.entries.contains_key(&canonical) {
            return Err(Error::Exists(canonical));
        }
        self.entries.insert(canonical, Entry::Dir);
        Ok(())
    }

    pub fn write(&mut self, path: &str, data: &[u8]) -> Result<(), Error> {
        let canonical = self.normalize(path)?;
        self.resolve_parent(&canonical)?;
        match self.entries.get(&canonical) {
            Some(Entry::Dir) => Err(Error::IsDir(canonical)),
            Some(Entry::File(_)) | None => {
                self.entries
                    .insert(canonical, Entry::File(data.to_vec()));
                Ok(())
            }
        }
    }

    pub fn read(&self, path: &str) -> Result<Vec<u8>, Error> {
        let canonical = self.normalize(path)?;
        match self.resolve_existing(&canonical)? {
            Entry::Dir => Err(Error::IsDir(canonical)),
            Entry::File(data) => Ok(data.clone()),
        }
    }

    pub fn ls(&self, path: &str) -> Result<Vec<(String, Kind)>, Error> {
        let canonical = self.normalize(path)?;
        match self.resolve_existing(&canonical)? {
            Entry::File(_) => return Err(Error::NotDir(canonical)),
            Entry::Dir => {}
        }

        let prefix = if canonical == "/" {
            "/".to_string()
        } else {
            format!("{}/", canonical)
        };
        let mut children = Vec::new();
        for (entry_path, entry) in &self.entries {
            if entry_path == &canonical || !entry_path.starts_with(&prefix) {
                continue;
            }
            let rest = &entry_path[prefix.len()..];
            if !rest.contains('/') {
                children.push((rest.to_string(), entry.kind()));
            }
        }
        children.sort_by(|a, b| a.0.cmp(&b.0));
        Ok(children)
    }

    pub fn cwd(&self) -> String {
        self.cwd.clone()
    }

    pub fn chdir(&mut self, path: &str) -> Result<(), Error> {
        let canonical = self.normalize(path)?;
        match self.resolve_existing(&canonical)? {
            Entry::Dir => {
                self.cwd = canonical;
                Ok(())
            }
            Entry::File(_) => Err(Error::NotDir(canonical)),
        }
    }

    pub fn rename(&mut self, from: &str, to: &str, overwrite: bool) -> Result<(), Error> {
        let source = self.normalize(from)?;
        let destination = self.normalize(to)?;

        if source == "/" || destination == "/" {
            return Err(Error::Root);
        }

        self.resolve_parent(&source)?;
        let source_entry = self
            .entries
            .get(&source)
            .cloned()
            .ok_or_else(|| Error::Missing(source.clone()))?;

        if matches!(source_entry, Entry::Dir) && Self::is_within(&self.cwd, &source) {
            return Err(Error::Busy(source));
        }

        self.resolve_parent(&destination)?;

        if matches!(self.entries.get(&destination), Some(Entry::Dir))
            && Self::is_within(&self.cwd, &destination)
        {
            return Err(Error::Busy(destination));
        }

        if source == destination {
            return Ok(());
        }

        if matches!(source_entry, Entry::Dir) && Self::is_within(&destination, &source) {
            return Err(Error::Cycle(destination));
        }

        let destination_entry = self.entries.get(&destination).cloned();
        if destination_entry.is_some() && !overwrite {
            return Err(Error::Exists(destination));
        }

        if overwrite {
            if let Some(existing) = &destination_entry {
                match (&source_entry, existing) {
                    (Entry::Dir, Entry::File(_)) => {
                        return Err(Error::NotDir(destination));
                    }
                    (Entry::File(_), Entry::Dir) => {
                        return Err(Error::IsDir(destination));
                    }
                    (Entry::Dir, Entry::Dir) => {
                        if self.subtree_keys(&destination).len() > 1 {
                            return Err(Error::NotEmpty(destination));
                        }
                    }
                    _ => {}
                }
            }
        }

        let source_keys = self.subtree_keys(&source);
        let mut moved = Vec::with_capacity(source_keys.len());
        for old_path in &source_keys {
            let suffix = &old_path[source.len()..];
            let new_path = format!("{}{}", destination, suffix);
            let entry = self.entries.get(old_path).cloned().expect("entry exists");
            moved.push((old_path.clone(), new_path, entry));
        }

        if destination_entry.is_some() {
            for key in self.subtree_keys(&destination) {
                self.entries.remove(&key);
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
        let canonical = self.normalize(path)?;
        if canonical == "/" {
            return Err(Error::Root);
        }

        self.resolve_parent(&canonical)?;
        let entry = self
            .entries
            .get(&canonical)
            .ok_or_else(|| Error::Missing(canonical.clone()))?;

        if matches!(entry, Entry::Dir) && Self::is_within(&self.cwd, &canonical) {
            return Err(Error::Busy(canonical));
        }

        let keys = self.subtree_keys(&canonical);
        if matches!(entry, Entry::Dir) && keys.len() > 1 && !recursive {
            return Err(Error::NotEmpty(canonical));
        }

        for key in keys {
            self.entries.remove(&key);
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