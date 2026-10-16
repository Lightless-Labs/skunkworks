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

impl Fs {
    pub fn new() -> Self {
        let mut entries = BTreeMap::new();
        entries.insert("/".to_owned(), Entry::Dir);
        Self {
            entries,
            cwd: "/".to_owned(),
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
        let mut parts: Vec<String> = if absolute || self.cwd == "/" {
            Vec::new()
        } else {
            self.cwd[1..].split('/').map(str::to_owned).collect()
        };

        for component in path.split('/') {
            match component {
                "" | "." => {}
                ".." => {
                    parts.pop();
                }
                other => parts.push(other.to_owned()),
            }
        }

        if parts.is_empty() {
            Ok("/".to_owned())
        } else {
            Ok(format!("/{}", parts.join("/")))
        }
    }

    fn parent_and_name(path: &str) -> Option<(String, String)> {
        if path == "/" {
            return None;
        }
        match path.rfind('/') {
            Some(0) => Some(("/".to_owned(), path[1..].to_owned())),
            Some(i) => Some((path[..i].to_owned(), path[i + 1..].to_owned())),
            None => None,
        }
    }

    fn lookup(&self, path: &str) -> Result<&Entry, Error> {
        if path == "/" {
            return self
                .entries
                .get("/")
                .ok_or_else(|| Error::Missing("/".to_owned()));
        }

        let mut prefix = String::new();
        let components: Vec<&str> = path[1..].split('/').collect();
        for (i, component) in components.iter().enumerate() {
            prefix.push('/');
            prefix.push_str(component);
            match self.entries.get(&prefix) {
                None => return Err(Error::Missing(prefix)),
                Some(entry) if i + 1 < components.len() && !matches!(entry, Entry::Dir) => {
                    return Err(Error::NotDir(prefix));
                }
                Some(entry) if i + 1 == components.len() => return Ok(entry),
                Some(_) => {}
            }
        }
        Err(Error::Missing(path.to_owned()))
    }

    fn resolve_parent(&self, path: &str) -> Result<(String, String), Error> {
        let (parent, name) =
            Self::parent_and_name(path).ok_or_else(|| Error::Root)?;
        match self.lookup(&parent)? {
            Entry::Dir => Ok((parent, name)),
            Entry::File(_) => Err(Error::NotDir(parent)),
        }
    }

    fn contains_path(directory: &str, path: &str) -> bool {
        if directory == "/" {
            return true;
        }
        path == directory
            || path
                .strip_prefix(directory)
                .is_some_and(|suffix| suffix.starts_with('/'))
    }

    fn has_children(&self, path: &str) -> bool {
        self.entries
            .keys()
            .any(|candidate| candidate != path && Self::contains_path(path, candidate))
    }

    fn remove_tree(&mut self, path: &str) {
        self.entries
            .retain(|candidate, _| !Self::contains_path(path, candidate));
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
            Some(Entry::Dir) => Err(Error::IsDir(canonical)),
            Some(Entry::File(_)) => {
                self.entries
                    .insert(canonical, Entry::File(data.to_vec()));
                Ok(())
            }
            None => {
                self.entries
                    .insert(canonical, Entry::File(data.to_vec()));
                Ok(())
            }
        }
    }

    pub fn read(&self, path: &str) -> Result<Vec<u8>, Error> {
        let canonical = self.validate_and_normalize(path)?;
        match self.lookup(&canonical)? {
            Entry::File(data) => Ok(data.clone()),
            Entry::Dir => Err(Error::IsDir(canonical)),
        }
    }

    pub fn ls(&self, path: &str) -> Result<Vec<(String, Kind)>, Error> {
        let canonical = self.validate_and_normalize(path)?;
        match self.lookup(&canonical)? {
            Entry::File(_) => return Err(Error::NotDir(canonical)),
            Entry::Dir => {}
        }

        let mut children = Vec::new();
        for (candidate, entry) in &self.entries {
            if candidate == &canonical {
                continue;
            }
            if let Some((parent, name)) = Self::parent_and_name(candidate) {
                if parent == canonical {
                    let kind = match entry {
                        Entry::Dir => Kind::Dir,
                        Entry::File(_) => Kind::File,
                    };
                    children.push((name, kind));
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
        match self.lookup(&canonical)? {
            Entry::Dir => {
                self.cwd = canonical;
                Ok(())
            }
            Entry::File(_) => Err(Error::NotDir(canonical)),
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

        let (source_parent, _) = Self::parent_and_name(&source)
            .ok_or(Error::Root)?;
        self.lookup(&source_parent).and_then(|entry| match entry {
            Entry::Dir => Ok(()),
            Entry::File(_) => Err(Error::NotDir(source_parent.clone())),
        })?;

        let source_entry = self
            .entries
            .get(&source)
            .cloned()
            .ok_or_else(|| Error::Missing(source.clone()))?;

        if matches!(source_entry, Entry::Dir) && Self::contains_path(&source, &self.cwd) {
            return Err(Error::Busy(source));
        }

        let (destination_parent, _) = Self::parent_and_name(&destination)
            .ok_or(Error::Root)?;
        self.lookup(&destination_parent).and_then(|entry| match entry {
            Entry::Dir => Ok(()),
            Entry::File(_) => Err(Error::NotDir(destination_parent.clone())),
        })?;

        if matches!(self.entries.get(&destination), Some(Entry::Dir))
            && Self::contains_path(&destination, &self.cwd)
        {
            return Err(Error::Busy(destination));
        }

        if source == destination {
            return Ok(());
        }

        if matches!(source_entry, Entry::Dir) && Self::contains_path(&source, &destination) {
            return Err(Error::Cycle(destination));
        }

        if self.entries.contains_key(&destination) {
            if !overwrite {
                return Err(Error::Exists(destination));
            }

            match (&source_entry, self.entries.get(&destination).unwrap()) {
                (Entry::Dir, Entry::File(_)) => return Err(Error::NotDir(destination)),
                (Entry::File(_), Entry::Dir) => return Err(Error::IsDir(destination)),
                (Entry::Dir, Entry::Dir) if self.has_children(&destination) => {
                    return Err(Error::NotEmpty(destination));
                }
                _ => {}
            }
        }

        let moved: Vec<(String, Entry)> = self
            .entries
            .iter()
            .filter(|(candidate, _)| Self::contains_path(&source, candidate))
            .map(|(candidate, entry)| (candidate.clone(), entry.clone()))
            .collect();

        if self.entries.contains_key(&destination) {
            self.remove_tree(&destination);
        }
        self.remove_tree(&source);

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

        let entry = self
            .entries
            .get(&canonical)
            .ok_or_else(|| Error::Missing(canonical.clone()))?;

        if matches!(entry, Entry::Dir) && Self::contains_path(&canonical, &self.cwd) {
            return Err(Error::Busy(canonical));
        }

        if matches!(entry, Entry::Dir) && self.has_children(&canonical) && !recursive {
            return Err(Error::NotEmpty(canonical));
        }

        self.remove_tree(&canonical);
        Ok(())
    }
}