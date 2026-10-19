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

    fn normalize(&self, path: &str) -> Result<String, Error> {
        if let Some(index) = path.as_bytes().iter().position(|&byte| byte == 0) {
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
            Ok("/".to_owned())
        } else {
            Ok(format!("/{}", components.join("/")))
        }
    }

    fn parent(path: &str) -> (&str, &str) {
        match path.rfind('/') {
            Some(0) => ("/", &path[1..]),
            Some(index) => (&path[..index], &path[index + 1..]),
            _ => ("/", path),
        }
    }

    fn within(path: &str, directory: &str) -> bool {
        if directory == "/" {
            return path.starts_with('/');
        }
        path == directory
            || (path.starts_with(directory)
                && path.as_bytes().get(directory.len()) == Some(&b'/'))
    }

    fn resolve_parent(&self, parent: &str) -> Result<(), Error> {
        let mut prefix = String::new();

        for component in parent.split('/').filter(|part| !part.is_empty()) {
            prefix.push('/');
            prefix.push_str(component);

            match self.entries.get(&prefix) {
                None => return Err(Error::Missing(prefix)),
                Some(Entry::File(_)) => return Err(Error::NotDir(prefix)),
                Some(Entry::Dir) => {}
            }
        }

        Ok(())
    }

    fn resolve(&self, path: &str) -> Result<&Entry, Error> {
        if path == "/" {
            return self
                .entries
                .get("/")
                .ok_or_else(|| Error::Missing("/".to_owned()));
        }

        let mut prefix = String::new();
        let components: Vec<&str> = path.split('/').filter(|part| !part.is_empty()).collect();

        for (index, component) in components.iter().enumerate() {
            prefix.push('/');
            prefix.push_str(component);

            match self.entries.get(&prefix) {
                None => return Err(Error::Missing(prefix)),
                Some(entry) if index + 1 < components.len() && !matches!(entry, Entry::Dir) => {
                    return Err(Error::NotDir(prefix));
                }
                Some(entry) if index + 1 == components.len() => return Ok(entry),
                Some(_) => {}
            }
        }

        Err(Error::Missing(path.to_owned()))
    }

    fn kind(entry: &Entry) -> Kind {
        match entry {
            Entry::Dir => Kind::Dir,
            Entry::File(_) => Kind::File,
        }
    }

    pub fn mkdir(&mut self, path: &str) -> Result<(), Error> {
        let path = self.normalize(path)?;
        if path == "/" {
            return Err(Error::Exists(path));
        }

        let (parent, _) = Self::parent(&path);
        self.resolve_parent(parent)?;

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

        let (parent, _) = Self::parent(&path);
        self.resolve_parent(parent)?;

        match self.entries.get(&path) {
            Some(Entry::Dir) => return Err(Error::IsDir(path)),
            Some(Entry::File(_)) | None => {}
        }

        self.entries.insert(path, Entry::File(data.to_vec()));
        Ok(())
    }

    pub fn read(&self, path: &str) -> Result<Vec<u8>, Error> {
        let path = self.normalize(path)?;
        match self.resolve(&path)? {
            Entry::File(data) => Ok(data.clone()),
            Entry::Dir => Err(Error::IsDir(path)),
        }
    }

    pub fn ls(&self, path: &str) -> Result<Vec<(String, Kind)>, Error> {
        let path = self.normalize(path)?;
        match self.resolve(&path)? {
            Entry::File(_) => return Err(Error::NotDir(path)),
            Entry::Dir => {}
        }

        let mut children = Vec::new();
        for (child_path, entry) in &self.entries {
            if child_path == "/" {
                continue;
            }
            let (parent, name) = Self::parent(child_path);
            if parent == path {
                children.push((name.to_owned(), Self::kind(entry)));
            }
        }
        children.sort_by(|left, right| left.0.cmp(&right.0));
        Ok(children)
    }

    pub fn cwd(&self) -> String {
        self.cwd.clone()
    }

    pub fn chdir(&mut self, path: &str) -> Result<(), Error> {
        let path = self.normalize(path)?;
        match self.resolve(&path)? {
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

        let (source_parent, _) = Self::parent(&from);
        self.resolve_parent(source_parent)?;
        let source = self
            .entries
            .get(&from)
            .cloned()
            .ok_or_else(|| Error::Missing(from.clone()))?;

        if matches!(source, Entry::Dir) && Self::within(&self.cwd, &from) {
            return Err(Error::Busy(from));
        }

        let (destination_parent, _) = Self::parent(&to);
        self.resolve_parent(destination_parent)?;

        if matches!(self.entries.get(&to), Some(Entry::Dir))
            && Self::within(&self.cwd, &to)
        {
            return Err(Error::Busy(to));
        }

        if from == to {
            return Ok(());
        }

        if matches!(source, Entry::Dir) && Self::within(&to, &from) {
            return Err(Error::Cycle(to));
        }

        let destination = self.entries.get(&to).cloned();
        if destination.is_some() && !overwrite {
            return Err(Error::Exists(to));
        }

        if let Some(existing) = &destination {
            match (&source, existing) {
                (Entry::Dir, Entry::File(_)) => return Err(Error::NotDir(to)),
                (Entry::File(_), Entry::Dir) => return Err(Error::IsDir(to)),
                (Entry::Dir, Entry::Dir) => {
                    if self
                        .entries
                        .keys()
                        .any(|path| path != &to && Self::within(path, &to))
                    {
                        return Err(Error::NotEmpty(to));
                    }
                }
                _ => {}
            }
        }

        let mut moved = Vec::new();
        for (path, entry) in &self.entries {
            if path == &from || Self::within(path, &from) {
                let suffix = &path[from.len()..];
                moved.push((format!("{to}{suffix}"), entry.clone()));
            }
        }

        if destination.is_some() {
            self.entries
                .retain(|path, _| path != &to && !Self::within(path, &to));
        }
        self.entries
            .retain(|path, _| path != &from && !Self::within(path, &from));

        for (path, entry) in moved {
            self.entries.insert(path, entry);
        }

        Ok(())
    }

    pub fn remove(&mut self, path: &str, recursive: bool) -> Result<(), Error> {
        let path = self.normalize(path)?;
        if path == "/" {
            return Err(Error::Root);
        }

        let (parent, _) = Self::parent(&path);
        self.resolve_parent(parent)?;
        let entry = self
            .entries
            .get(&path)
            .ok_or_else(|| Error::Missing(path.clone()))?;

        if matches!(entry, Entry::Dir) && Self::within(&self.cwd, &path) {
            return Err(Error::Busy(path));
        }

        if matches!(entry, Entry::Dir)
            && !recursive
            && self
                .entries
                .keys()
                .any(|child| child != &path && Self::within(child, &path))
        {
            return Err(Error::NotEmpty(path));
        }

        self.entries
            .retain(|child, _| child != &path && !Self::within(child, &path));
        Ok(())
    }
}

impl Default for Fs {
    fn default() -> Self {
        Self::new()
    }
}