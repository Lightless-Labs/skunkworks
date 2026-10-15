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
}

enum Entry {
    Dir,
    File(Vec<u8>),
}

pub struct Fs {
    entries: BTreeMap<String, Entry>,
    cwd: Vec<String>,
}

impl Fs {
    pub fn new() -> Self {
        let mut entries = BTreeMap::new();
        entries.insert("/".to_owned(), Entry::Dir);
        Self {
            entries,
            cwd: Vec::new(),
        }
    }

    fn normalize(&self, path: &str) -> Result<Vec<String>, Error> {
        if let Some(i) = path.as_bytes().iter().position(|&b| b == 0) {
            return Err(Error::InvalidPath(i));
        }
        if path.is_empty() {
            return Err(Error::InvalidPath(0));
        }

        let mut components = if path.starts_with('/') {
            Vec::new()
        } else {
            self.cwd.clone()
        };

        for component in path.split('/') {
            match component {
                "" | "." => {}
                ".." => {
                    components.pop();
                }
                name => components.push(name.to_owned()),
            }
        }

        Ok(components)
    }

    fn canonical(components: &[String]) -> String {
        if components.is_empty() {
            "/".to_owned()
        } else {
            format!("/{}", components.join("/"))
        }
    }

    fn lookup_parent(&self, components: &[String]) -> Result<(), Error> {
        let mut prefix = Vec::new();

        for component in components.iter().take(components.len().saturating_sub(1)) {
            prefix.push(component.clone());
            let path = Self::canonical(&prefix);
            match self.entries.get(&path) {
                None => return Err(Error::Missing(path)),
                Some(Entry::Dir) => {}
                Some(Entry::File(_)) => return Err(Error::NotDir(path)),
            }
        }

        Ok(())
    }

    pub fn mkdir(&mut self, path: &str) -> Result<(), Error> {
        let components = self.normalize(path)?;
        let canonical = Self::canonical(&components);
        if components.is_empty() {
            return Err(Error::Exists(canonical));
        }

        self.lookup_parent(&components)?;
        let parent = Self::canonical(&components[..components.len() - 1]);
        match self.entries.get(&parent) {
            None => return Err(Error::Missing(parent)),
            Some(Entry::Dir) => {}
            Some(Entry::File(_)) => return Err(Error::NotDir(parent)),
        }

        if self.entries.contains_key(&canonical) {
            return Err(Error::Exists(canonical));
        }

        self.entries.insert(canonical, Entry::Dir);
        Ok(())
    }

    pub fn write(&mut self, path: &str, data: &[u8]) -> Result<(), Error> {
        let components = self.normalize(path)?;
        let canonical = Self::canonical(&components);
        if components.is_empty() {
            return Err(Error::IsDir(canonical));
        }

        self.lookup_parent(&components)?;
        let parent = Self::canonical(&components[..components.len() - 1]);
        match self.entries.get(&parent) {
            None => return Err(Error::Missing(parent)),
            Some(Entry::Dir) => {}
            Some(Entry::File(_)) => return Err(Error::NotDir(parent)),
        }

        match self.entries.get_mut(&canonical) {
            Some(Entry::Dir) => Err(Error::IsDir(canonical)),
            Some(Entry::File(contents)) => {
                contents.clear();
                contents.extend_from_slice(data);
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
        let components = self.normalize(path)?;
        let canonical = Self::canonical(&components);
        if components.is_empty() {
            return Err(Error::IsDir(canonical));
        }

        self.lookup_parent(&components)?;
        match self.entries.get(&canonical) {
            None => Err(Error::Missing(canonical)),
            Some(Entry::Dir) => Err(Error::IsDir(canonical)),
            Some(Entry::File(contents)) => Ok(contents.clone()),
        }
    }

    pub fn ls(&self, path: &str) -> Result<Vec<(String, Kind)>, Error> {
        let components = self.normalize(path)?;
        let canonical = Self::canonical(&components);

        if !matches!(self.entries.get(&canonical), Some(Entry::Dir)) {
            return match self.entries.get(&canonical) {
                None => {
                    self.lookup_parent(&components)?;
                    Err(Error::Missing(canonical))
                }
                Some(Entry::File(_)) => {
                    self.lookup_parent(&components)?;
                    Err(Error::NotDir(canonical))
                }
                Some(Entry::Dir) => unreachable!(),
            };
        }

        let mut result = Vec::new();
        for (entry_path, entry) in &self.entries {
            if entry_path == "/" {
                continue;
            }
            let Some((parent, basename)) = entry_path.rsplit_once('/') else {
                continue;
            };
            let parent = if parent.is_empty() { "/" } else { parent };
            if parent == canonical {
                let kind = match entry {
                    Entry::Dir => Kind::Dir,
                    Entry::File(_) => Kind::File,
                };
                result.push((basename.to_owned(), kind));
            }
        }
        result.sort_by(|a, b| a.0.cmp(&b.0));
        Ok(result)
    }

    pub fn cwd(&self) -> String {
        Self::canonical(&self.cwd)
    }

    pub fn chdir(&mut self, path: &str) -> Result<(), Error> {
        let components = self.normalize(path)?;
        let canonical = Self::canonical(&components);
        self.lookup_parent(&components)?;

        match self.entries.get(&canonical) {
            None => Err(Error::Missing(canonical)),
            Some(Entry::File(_)) => Err(Error::NotDir(canonical)),
            Some(Entry::Dir) => {
                self.cwd = components;
                Ok(())
            }
        }
    }
}