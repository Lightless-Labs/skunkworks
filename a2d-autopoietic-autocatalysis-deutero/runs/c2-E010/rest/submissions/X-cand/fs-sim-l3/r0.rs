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

impl Default for Fs {
    fn default() -> Self {
        Self::new()
    }
}

impl Fs {
    pub fn new() -> Self {
        let mut entries = BTreeMap::new();
        entries.insert("/".to_string(), Entry::Dir);
        Self { entries, cwd: "/".to_string() }
    }

    fn canonical(parts: &[String]) -> String {
        if parts.is_empty() {
            "/".to_string()
        } else {
            format!("/{}", parts.join("/"))
        }
    }

    fn validate_normalize(&self, path: &str) -> Result<String, Error> {
        if let Some(i) = path.as_bytes().iter().position(|&b| b == 0) {
            return Err(Error::InvalidPath(i));
        }
        if path.is_empty() {
            return Err(Error::InvalidPath(0));
        }

        let mut parts: Vec<String> = if path.starts_with('/') {
            Vec::new()
        } else if self.cwd == "/" {
            Vec::new()
        } else {
            self.cwd[1..].split('/').map(str::to_string).collect()
        };

        for component in path.split('/') {
            match component {
                "" | "." => {}
                ".." => {
                    parts.pop();
                }
                other => parts.push(other.to_string()),
            }
        }
        Ok(Self::canonical(&parts))
    }

    fn parent(path: &str) -> String {
        match path.rfind('/') {
            Some(0) | None => "/".to_string(),
            Some(i) => path[..i].to_string(),
        }
    }

    fn basename(path: &str) -> &str {
        path.rsplit('/').next().unwrap_or("")
    }

    fn resolve_dir(&self, path: &str) -> Result<(), Error> {
        if path == "/" {
            return Ok(());
        }
        let mut current = String::new();
        let parts: Vec<&str> = path[1..].split('/').collect();
        for (i, part) in parts.iter().enumerate() {
            current.push('/');
            current.push_str(part);
            match self.entries.get(&current) {
                None => return Err(Error::Missing(current)),
                Some(Entry::Dir) => {}
                Some(Entry::File(_)) => {
                    if i + 1 < parts.len() {
                        return Err(Error::NotDir(current));
                    }
                    return Err(Error::NotDir(current));
                }
            }
        }
        Ok(())
    }

    fn resolve_parent(&self, path: &str) -> Result<(), Error> {
        let parent = Self::parent(path);
        self.resolve_dir(&parent)
    }

    fn is_within(path: &str, ancestor: &str) -> bool {
        if ancestor == "/" {
            return true;
        }
        path == ancestor
            || path.strip_prefix(ancestor).is_some_and(|rest| rest.starts_with('/'))
    }

    fn children(&self, path: &str) -> Vec<String> {
        self.entries
            .keys()
            .filter_map(|candidate| {
                if candidate == "/" || Self::parent(candidate) != path {
                    None
                } else {
                    Some(candidate.clone())
                }
            })
            .collect()
    }

    pub fn mkdir(&mut self, path: &str) -> Result<(), Error> {
        let path = self.validate_normalize(path)?;
        self.resolve_parent(&path)?;
        if self.entries.contains_key(&path) {
            return Err(Error::Exists(path));
        }
        self.entries.insert(path, Entry::Dir);
        Ok(())
    }

    pub fn write(&mut self, path: &str, data: &[u8]) -> Result<(), Error> {
        let path = self.validate_normalize(path)?;
        self.resolve_parent(&path)?;
        match self.entries.get(&path) {
            Some(Entry::Dir) => return Err(Error::IsDir(path)),
            Some(Entry::File(_)) | None => {}
        }
        self.entries.insert(path, Entry::File(data.to_vec()));
        Ok(())
    }

    pub fn read(&self, path: &str) -> Result<Vec<u8>, Error> {
        let path = self.validate_normalize(path)?;
        self.resolve_parent(&path)?;
        match self.entries.get(&path) {
            None => Err(Error::Missing(path)),
            Some(Entry::Dir) => Err(Error::IsDir(path)),
            Some(Entry::File(bytes)) => Ok(bytes.clone()),
        }
    }

    pub fn ls(&self, path: &str) -> Result<Vec<(String, Kind)>, Error> {
        let path = self.validate_normalize(path)?;
        self.resolve_dir(&path)?;
        let mut result = Vec::new();
        for child in self.children(&path) {
            let kind = match self.entries.get(&child).expect("child key exists") {
                Entry::Dir => Kind::Dir,
                Entry::File(_) => Kind::File,
            };
            result.push((Self::basename(&child).to_string(), kind));
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
        let source = self.entries.get(&from).cloned().ok_or_else(|| Error::Missing(from.clone()))?;

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

        if let Some(dest) = &destination {
            match (&source, dest) {
                (Entry::Dir, Entry::File(_)) => return Err(Error::NotDir(to)),
                (Entry::File(_), Entry::Dir) => return Err(Error::IsDir(to)),
                (Entry::Dir, Entry::Dir) if !self.children(&to).is_empty() => {
                    return Err(Error::NotEmpty(to));
                }
                _ => {}
            }
        }

        let moving: Vec<(String, Entry)> = self.entries
            .iter()
            .filter_map(|(key, entry)| {
                if Self::is_within(key, &from) {
                    let suffix = &key[from.len()..];
                    Some((format!("{to}{suffix}"), entry.clone()))
                } else {
                    None
                }
            })
            .collect();

        if destination.is_some() {
            self.entries.retain(|key, _| !Self::is_within(key, &to));
        }
        self.entries.retain(|key, _| !Self::is_within(key, &from));
        for (key, entry) in moving {
            self.entries.insert(key, entry);
        }
        Ok(())
    }

    pub fn remove(&mut self, path: &str, recursive: bool) -> Result<(), Error> {
        let path = self.validate_normalize(path)?;
        if path == "/" {
            return Err(Error::Root);
        }
        self.resolve_parent(&path)?;
        let entry = self.entries.get(&path).ok_or_else(|| Error::Missing(path.clone()))?;

        if matches!(entry, Entry::Dir) && Self::is_within(&self.cwd, &path) {
            return Err(Error::Busy(path));
        }

        if matches!(entry, Entry::Dir) && !recursive && !self.children(&path).is_empty() {
            return Err(Error::NotEmpty(path));
        }

        self.entries.retain(|key, _| !Self::is_within(key, &path));
        Ok(())
    }
}
