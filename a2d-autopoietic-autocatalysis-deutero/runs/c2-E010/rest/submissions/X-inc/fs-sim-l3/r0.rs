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
        if let Some(index) = path.as_bytes().iter().position(|&b| b == 0) {
            return Err(Error::InvalidPath(index));
        }
        if path.is_empty() {
            return Err(Error::InvalidPath(0));
        }

        let mut components: Vec<&str> = Vec::new();
        if !path.starts_with('/') {
            components.extend(self.cwd.split('/').filter(|part| !part.is_empty()));
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

    fn components(path: &str) -> impl Iterator<Item = &str> {
        path.split('/').filter(|part| !part.is_empty())
    }

    fn parent_and_name(path: &str) -> (String, String) {
        let mut components: Vec<&str> = Self::components(path).collect();
        let name = components.pop().unwrap_or("").to_string();
        let parent = if components.is_empty() {
            "/".to_string()
        } else {
            format!("/{}", components.join("/"))
        };
        (parent, name)
    }

    fn resolve_dir(&self, path: &str) -> Result<(), Error> {
        let mut prefix = String::new();
        for component in Self::components(path) {
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

    fn resolve_entry(&self, path: &str) -> Result<&Entry, Error> {
        let (parent, _) = Self::parent_and_name(path);
        self.resolve_dir(&parent)?;
        self.entries
            .get(path)
            .ok_or_else(|| Error::Missing(path.to_string()))
    }

    fn is_within(path: &str, ancestor: &str) -> bool {
        if ancestor == "/" {
            return path.starts_with('/');
        }
        path == ancestor
            || path
                .strip_prefix(ancestor)
                .is_some_and(|rest| rest.starts_with('/'))
    }

    fn children(&self, path: &str) -> Vec<(String, Kind)> {
        self.entries
            .iter()
            .filter_map(|(candidate, entry)| {
                if candidate == path {
                    return None;
                }
                let (parent, name) = Self::parent_and_name(candidate);
                if parent == path {
                    Some((name, entry.kind()))
                } else {
                    None
                }
            })
            .collect()
    }

    pub fn mkdir(&mut self, path: &str) -> Result<(), Error> {
        let canonical = self.validate_and_normalize(path)?;
        if canonical == "/" {
            return Err(Error::Exists(canonical));
        }
        let (parent, _) = Self::parent_and_name(&canonical);
        self.resolve_dir(&parent)?;
        if self.entries.contains_key(&canonical) {
            return Err(Error::Exists(canonical));
        }
        self.entries.insert(canonical, Entry::Dir);
        Ok(())
    }

    pub fn write(&mut self, path: &str, data: &[u8]) -> Result<(), Error> {
        let canonical = self.validate_and_normalize(path)?;
        if canonical == "/" {
            return Err(Error::IsDir(canonical));
        }
        let (parent, _) = Self::parent_and_name(&canonical);
        self.resolve_dir(&parent)?;
        match self.entries.get(&canonical) {
            Some(Entry::Dir) => return Err(Error::IsDir(canonical)),
            Some(Entry::File(_)) | None => {}
        }
        self.entries
            .insert(canonical, Entry::File(data.to_vec()));
        Ok(())
    }

    pub fn read(&self, path: &str) -> Result<Vec<u8>, Error> {
        let canonical = self.validate_and_normalize(path)?;
        match self.resolve_entry(&canonical)? {
            Entry::File(data) => Ok(data.clone()),
            Entry::Dir => Err(Error::IsDir(canonical)),
        }
    }

    pub fn ls(&self, path: &str) -> Result<Vec<(String, Kind)>, Error> {
        let canonical = self.validate_and_normalize(path)?;
        match self.resolve_entry(&canonical)? {
            Entry::Dir => Ok(self.children(&canonical)),
            Entry::File(_) => Err(Error::NotDir(canonical)),
        }
    }

    pub fn cwd(&self) -> String {
        self.cwd.clone()
    }

    pub fn chdir(&mut self, path: &str) -> Result<(), Error> {
        let canonical = self.validate_and_normalize(path)?;
        match self.resolve_entry(&canonical)? {
            Entry::Dir => {
                self.cwd = canonical;
                Ok(())
            }
            Entry::File(_) => Err(Error::NotDir(canonical)),
        }
    }

    pub fn rename(&mut self, from: &str, to: &str, overwrite: bool) -> Result<(), Error> {
        let source = self.validate_and_normalize(from)?;
        let destination = self.validate_and_normalize(to)?;

        if source == "/" || destination == "/" {
            return Err(Error::Root);
        }

        let (source_parent, _) = Self::parent_and_name(&source);
        self.resolve_dir(&source_parent)?;
        let source_entry = self
            .entries
            .get(&source)
            .cloned()
            .ok_or_else(|| Error::Missing(source.clone()))?;

        if matches!(source_entry, Entry::Dir) && Self::is_within(&self.cwd, &source) {
            return Err(Error::Busy(source));
        }

        let (destination_parent, _) = Self::parent_and_name(&destination);
        self.resolve_dir(&destination_parent)?;

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

        if let Some(dest_entry) = destination_entry.as_ref() {
            match (&source_entry, dest_entry) {
                (Entry::Dir, Entry::File(_)) => return Err(Error::NotDir(destination)),
                (Entry::File(_), Entry::Dir) => return Err(Error::IsDir(destination)),
                (Entry::Dir, Entry::Dir) => {
                    if !self.children(&destination).is_empty() {
                        return Err(Error::NotEmpty(destination));
                    }
                }
                _ => {}
            }
        }

        let moved: Vec<(String, Entry)> = self
            .entries
            .iter()
            .filter(|(path, _)| Self::is_within(path, &source))
            .map(|(path, entry)| (path.clone(), entry.clone()))
            .collect();

        if destination_entry.is_some() {
            self.entries
                .retain(|path, _| !Self::is_within(path, &destination));
        }
        self.entries
            .retain(|path, _| !Self::is_within(path, &source));

        for (old_path, entry) in moved {
            let suffix = &old_path[source.len()..];
            let new_path = format!("{}{}", destination, suffix);
            self.entries.insert(new_path, entry);
        }

        Ok(())
    }

    pub fn remove(&mut self, path: &str, recursive: bool) -> Result<(), Error> {
        let canonical = self.validate_and_normalize(path)?;
        if canonical == "/" {
            return Err(Error::Root);
        }

        let (parent, _) = Self::parent_and_name(&canonical);
        self.resolve_dir(&parent)?;
        let entry = self
            .entries
            .get(&canonical)
            .cloned()
            .ok_or_else(|| Error::Missing(canonical.clone()))?;

        if matches!(entry, Entry::Dir) && Self::is_within(&self.cwd, &canonical) {
            return Err(Error::Busy(canonical));
        }

        if matches!(entry, Entry::Dir)
            && !recursive
            && !self.children(&canonical).is_empty()
        {
            return Err(Error::NotEmpty(canonical));
        }

        self.entries
            .retain(|candidate, _| !Self::is_within(candidate, &canonical));
        Ok(())
    }
}

impl Default for Fs {
    fn default() -> Self {
        Self::new()
    }
}