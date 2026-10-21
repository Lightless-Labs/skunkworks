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
    entries: std::collections::BTreeMap<String, Entry>,
    cwd: String,
}

impl Fs {
    pub fn new() -> Self {
        let mut entries = std::collections::BTreeMap::new();
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

        let absolute = path.starts_with('/');
        let mut components: Vec<String> = if absolute {
            Vec::new()
        } else {
            self.cwd
                .split('/')
                .filter(|part| !part.is_empty())
                .map(str::to_string)
                .collect()
        };

        for component in path.split('/') {
            match component {
                "" | "." => {}
                ".." => {
                    components.pop();
                }
                other => components.push(other.to_string()),
            }
        }

        if components.is_empty() {
            Ok("/".to_string())
        } else {
            Ok(format!("/{}", components.join("/")))
        }
    }

    fn parent_and_name(path: &str) -> Option<(String, String)> {
        if path == "/" {
            return None;
        }
        let index = path.rfind('/')?;
        let name = path[index + 1..].to_string();
        let parent = if index == 0 {
            "/".to_string()
        } else {
            path[..index].to_string()
        };
        Some((parent, name))
    }

    fn resolve_directory(&self, path: &str) -> Result<(), Error> {
        let mut current = String::new();
        for component in path.split('/').filter(|part| !part.is_empty()) {
            current.push('/');
            current.push_str(component);
            match self.entries.get(&current) {
                None => return Err(Error::Missing(current)),
                Some(Entry::File(_)) => return Err(Error::NotDir(current)),
                Some(Entry::Dir) => {}
            }
        }
        Ok(())
    }

    fn resolve_parent(&self, path: &str) -> Result<(String, String), Error> {
        let (parent, name) = Self::parent_and_name(path).expect("non-root path");
        self.resolve_directory(&parent)?;
        Ok((parent, name))
    }

    fn is_same_or_descendant(path: &str, directory: &str) -> bool {
        if directory == "/" {
            return path.starts_with('/');
        }
        path == directory
            || (path.starts_with(directory)
                && path.as_bytes().get(directory.len()) == Some(&b'/'))
    }

    fn contains_cwd(&self, directory: &str) -> bool {
        Self::is_same_or_descendant(&self.cwd, directory)
    }

    pub fn mkdir(&mut self, path: &str) -> Result<(), Error> {
        let canonical = self.validate_and_normalize(path)?;
        if canonical == "/" {
            return Err(Error::Exists(canonical));
        }
        let (parent, _) = self.resolve_parent(&canonical)?;
        if self.entries.contains_key(&canonical) {
            return Err(Error::Exists(canonical));
        }
        let _ = parent;
        self.entries.insert(canonical, Entry::Dir);
        Ok(())
    }

    pub fn write(&mut self, path: &str, data: &[u8]) -> Result<(), Error> {
        let canonical = self.validate_and_normalize(path)?;
        if canonical == "/" {
            return Err(Error::IsDir(canonical));
        }
        self.resolve_parent(&canonical)?;
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
        if canonical == "/" {
            return Err(Error::IsDir(canonical));
        }
        self.resolve_parent(&canonical)?;
        match self.entries.get(&canonical) {
            None => Err(Error::Missing(canonical)),
            Some(Entry::Dir) => Err(Error::IsDir(canonical)),
            Some(Entry::File(data)) => Ok(data.clone()),
        }
    }

    pub fn ls(&self, path: &str) -> Result<Vec<(String, Kind)>, Error> {
        let canonical = self.validate_and_normalize(path)?;
        self.resolve_directory(&canonical)?;

        let mut children = Vec::new();
        for (entry_path, entry) in &self.entries {
            if entry_path == "/" {
                continue;
            }
            if let Some((parent, name)) = Self::parent_and_name(entry_path) {
                if parent == canonical {
                    children.push((name, entry.kind()));
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
        let canonical = self.validate_and_normalize(path)?;
        match self.entries.get(&canonical) {
            None => {
                if canonical == "/" {
                    return Err(Error::Missing("/".to_string()));
                }
                self.resolve_parent(&canonical)?;
                Err(Error::Missing(canonical))
            }
            Some(Entry::File(_)) => Err(Error::NotDir(canonical)),
            Some(Entry::Dir) => {
                self.cwd = canonical;
                Ok(())
            }
        }
    }

    pub fn rename(
        &mut self,
        from: &str,
        to: &str,
        overwrite: bool,
    ) -> Result<(), Error> {
        let source = self.validate_and_normalize(from)?;
        let destination = self.validate_and_normalize(to)?;

        if source == "/" || destination == "/" {
            return Err(Error::Root);
        }

        self.resolve_parent(&source)?;
        let source_entry = match self.entries.get(&source) {
            None => return Err(Error::Missing(source)),
            Some(entry) => entry.clone(),
        };

        if matches!(source_entry, Entry::Dir) && self.contains_cwd(&source) {
            return Err(Error::Busy(source));
        }

        self.resolve_parent(&destination)?;

        if matches!(self.entries.get(&destination), Some(Entry::Dir))
            && self.contains_cwd(&destination)
        {
            return Err(Error::Busy(destination));
        }

        if source == destination {
            return Ok(());
        }

        if matches!(source_entry, Entry::Dir)
            && Self::is_same_or_descendant(&destination, &source)
        {
            return Err(Error::Cycle(destination));
        }

        if let Some(destination_entry) = self.entries.get(&destination) {
            if !overwrite {
                return Err(Error::Exists(destination));
            }

            match (&source_entry, destination_entry) {
                (Entry::Dir, Entry::File(_)) => {
                    return Err(Error::NotDir(destination));
                }
                (Entry::File(_), Entry::Dir) => {
                    return Err(Error::IsDir(destination));
                }
                (Entry::Dir, Entry::Dir) => {
                    if self.entries.keys().any(|path| {
                        path != &destination
                            && Self::is_same_or_descendant(path, &destination)
                    }) {
                        return Err(Error::NotEmpty(destination));
                    }
                }
                (Entry::File(_), Entry::File(_)) => {}
            }
        }

        let moved: Vec<(String, Entry)> = self
            .entries
            .iter()
            .filter(|(path, _)| Self::is_same_or_descendant(path, &source))
            .map(|(path, entry)| (path.clone(), entry.clone()))
            .collect();

        if self.entries.contains_key(&destination) {
            self.entries.remove(&destination);
        }
        for (path, _) in &moved {
            self.entries.remove(path);
        }
        for (old_path, entry) in moved {
            let suffix = &old_path[source.len()..];
            let new_path = format!("{destination}{suffix}");
            self.entries.insert(new_path, entry);
        }
        Ok(())
    }

    pub fn remove(&mut self, path: &str, recursive: bool) -> Result<(), Error> {
        let canonical = self.validate_and_normalize(path)?;
        if canonical == "/" {
            return Err(Error::Root);
        }

        self.resolve_parent(&canonical)?;
        let entry = match self.entries.get(&canonical) {
            None => return Err(Error::Missing(canonical)),
            Some(entry) => entry.clone(),
        };

        if matches!(entry, Entry::Dir) && self.contains_cwd(&canonical) {
            return Err(Error::Busy(canonical));
        }

        let has_children = matches!(entry, Entry::Dir)
            && self
                .entries
                .keys()
                .any(|candidate| candidate != &canonical && Self::is_same_or_descendant(candidate, &canonical));

        if has_children && !recursive {
            return Err(Error::NotEmpty(canonical));
        }

        if recursive && matches!(entry, Entry::Dir) {
            let to_remove: Vec<String> = self
                .entries
                .keys()
                .filter(|candidate| Self::is_same_or_descendant(candidate, &canonical))
                .cloned()
                .collect();
            for candidate in to_remove {
                self.entries.remove(&candidate);
            }
        } else {
            self.entries.remove(&canonical);
        }
        Ok(())
    }
}