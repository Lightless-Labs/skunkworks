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

    fn components(path: &str) -> Vec<&str> {
        path.split('/').filter(|part| !part.is_empty()).collect()
    }

    fn canonical(parts: &[String]) -> String {
        if parts.is_empty() {
            "/".to_owned()
        } else {
            format!("/{}", parts.join("/"))
        }
    }

    fn normalize(&self, path: &str) -> Result<String, Error> {
        if let Some(index) = path.as_bytes().iter().position(|&byte| byte == 0) {
            return Err(Error::InvalidPath(index));
        }
        if path.is_empty() {
            return Err(Error::InvalidPath(0));
        }

        let mut parts: Vec<String> = if path.starts_with('/') {
            Vec::new()
        } else {
            Self::components(&self.cwd)
                .into_iter()
                .map(str::to_owned)
                .collect()
        };

        for part in path.split('/') {
            match part {
                "" | "." => {}
                ".." => {
                    parts.pop();
                }
                other => parts.push(other.to_owned()),
            }
        }
        Ok(Self::canonical(&parts))
    }

    fn parent_and_name(path: &str) -> (String, String) {
        let parts: Vec<&str> = Self::components(path);
        let name = parts.last().copied().unwrap_or("").to_owned();
        let parent_parts: Vec<String> = parts[..parts.len().saturating_sub(1)]
            .iter()
            .map(|part| (*part).to_owned())
            .collect();
        (Self::canonical(&parent_parts), name)
    }

    fn lookup(&self, path: &str) -> Result<&Entry, Error> {
        if path == "/" {
            return self
                .entries
                .get("/")
                .ok_or_else(|| Error::Missing("/".to_owned()));
        }

        let parts = Self::components(path);
        let mut prefix = String::new();

        for (index, part) in parts.iter().enumerate() {
            prefix.push('/');
            prefix.push_str(part);
            match self.entries.get(&prefix) {
                None => return Err(Error::Missing(prefix)),
                Some(entry) if index + 1 < parts.len() && !matches!(entry, Entry::Dir) => {
                    return Err(Error::NotDir(prefix));
                }
                Some(entry) if index + 1 == parts.len() => return Ok(entry),
                Some(_) => {}
            }
        }

        self.entries
            .get("/")
            .ok_or_else(|| Error::Missing("/".to_owned()))
    }

    fn check_parent(&self, path: &str) -> Result<(String, String), Error> {
        let (parent, name) = Self::parent_and_name(path);
        match self.lookup(&parent)? {
            Entry::Dir => Ok((parent, name)),
            Entry::File(_) => Err(Error::NotDir(parent)),
        }
    }

    fn contains(directory: &str, candidate: &str) -> bool {
        directory == "/"
            || candidate == directory
            || candidate
                .strip_prefix(directory)
                .is_some_and(|suffix| suffix.starts_with('/'))
    }

    fn child_paths(&self, directory: &str) -> Vec<String> {
        self.entries
            .keys()
            .filter(|path| {
                if path.as_str() == "/" || path.as_str() == directory {
                    return false;
                }
                Self::parent_and_name(path).0 == directory
            })
            .cloned()
            .collect()
    }

    fn remove_subtree(&mut self, path: &str) {
        let prefix = if path == "/" {
            "/".to_owned()
        } else {
            format!("{path}/")
        };
        self.entries
            .retain(|key, _| key != path && !key.starts_with(&prefix));
    }

    pub fn mkdir(&mut self, path: &str) -> Result<(), Error> {
        let canonical = self.normalize(path)?;
        if canonical == "/" {
            return Err(Error::Exists(canonical));
        }
        self.check_parent(&canonical)?;
        if self.entries.contains_key(&canonical) {
            return Err(Error::Exists(canonical));
        }
        self.entries.insert(canonical, Entry::Dir);
        Ok(())
    }

    pub fn write(&mut self, path: &str, data: &[u8]) -> Result<(), Error> {
        let canonical = self.normalize(path)?;
        if canonical == "/" {
            return Err(Error::IsDir(canonical));
        }
        self.check_parent(&canonical)?;
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
        match self.lookup(&canonical)? {
            Entry::File(bytes) => Ok(bytes.clone()),
            Entry::Dir => Err(Error::IsDir(canonical)),
        }
    }

    pub fn ls(&self, path: &str) -> Result<Vec<(String, Kind)>, Error> {
        let canonical = self.normalize(path)?;
        match self.lookup(&canonical)? {
            Entry::File(_) => return Err(Error::NotDir(canonical)),
            Entry::Dir => {}
        }

        let mut children: Vec<(String, Kind)> = self
            .entries
            .iter()
            .filter_map(|(child_path, entry)| {
                let (parent, name) = Self::parent_and_name(child_path);
                if parent == canonical && child_path != "/" {
                    let kind = match entry {
                        Entry::Dir => Kind::Dir,
                        Entry::File(_) => Kind::File,
                    };
                    Some((name, kind))
                } else {
                    None
                }
            })
            .collect();
        children.sort_by(|a, b| a.0.cmp(&b.0));
        Ok(children)
    }

    pub fn cwd(&self) -> String {
        self.cwd.clone()
    }

    pub fn chdir(&mut self, path: &str) -> Result<(), Error> {
        let canonical = self.normalize(path)?;
        match self.lookup(&canonical)? {
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

        let (source_parent, _) = Self::parent_and_name(&source);
        self.lookup(&source_parent)?;
        let source_entry = self
            .entries
            .get(&source)
            .cloned()
            .ok_or_else(|| Error::Missing(source.clone()))?;

        if matches!(source_entry, Entry::Dir) && Self::contains(&source, &self.cwd) {
            return Err(Error::Busy(source));
        }

        let (destination_parent, _) = Self::parent_and_name(&destination);
        self.lookup(&destination_parent)?;

        let destination_entry = self.entries.get(&destination).cloned();
        if matches!(destination_entry, Some(Entry::Dir))
            && Self::contains(&destination, &self.cwd)
        {
            return Err(Error::Busy(destination));
        }

        if source == destination {
            return Ok(());
        }

        if matches!(source_entry, Entry::Dir) && Self::contains(&source, &destination) {
            return Err(Error::Cycle(destination));
        }

        if let Some(dest_entry) = destination_entry.as_ref() {
            if !overwrite {
                return Err(Error::Exists(destination));
            }
            match (&source_entry, dest_entry) {
                (Entry::Dir, Entry::File(_)) => return Err(Error::NotDir(destination)),
                (Entry::File(_), Entry::Dir) => return Err(Error::IsDir(destination)),
                (Entry::Dir, Entry::Dir) if !self.child_paths(&destination).is_empty() => {
                    return Err(Error::NotEmpty(destination));
                }
                _ => {}
            }
        }

        let source_prefix = format!("{source}/");
        let moved: Vec<(String, Entry)> = self
            .entries
            .iter()
            .filter(|(path, _)| *path == &source || path.starts_with(&source_prefix))
            .map(|(path, entry)| (path.clone(), entry.clone()))
            .collect();

        if destination_entry.is_some() {
            self.remove_subtree(&destination);
        }
        self.remove_subtree(&source);

        for (old_path, entry) in moved {
            let suffix = &old_path[source.len()..];
            self.entries
                .insert(format!("{destination}{suffix}"), entry);
        }
        Ok(())
    }

    pub fn remove(&mut self, path: &str, recursive: bool) -> Result<(), Error> {
        let canonical = self.normalize(path)?;
        if canonical == "/" {
            return Err(Error::Root);
        }

        let (parent, _) = Self::parent_and_name(&canonical);
        self.lookup(&parent)?;
        let entry = self
            .entries
            .get(&canonical)
            .cloned()
            .ok_or_else(|| Error::Missing(canonical.clone()))?;

        if matches!(entry, Entry::Dir) && Self::contains(&canonical, &self.cwd) {
            return Err(Error::Busy(canonical));
        }
        if matches!(entry, Entry::Dir)
            && !recursive
            && !self.child_paths(&canonical).is_empty()
        {
            return Err(Error::NotEmpty(canonical));
        }

        self.remove_subtree(&canonical);
        Ok(())
    }
}

fn main() {}