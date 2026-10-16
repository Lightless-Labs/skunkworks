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

    fn canonical(components: &[String]) -> String {
        if components.is_empty() {
            "/".to_owned()
        } else {
            format!("/{}", components.join("/"))
        }
    }

    fn normalize(&self, path: &str) -> Result<Vec<String>, Error> {
        if let Some(index) = path.as_bytes().iter().position(|&byte| byte == 0) {
            return Err(Error::InvalidPath(index));
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

    fn lookup(&self, components: &[String]) -> Result<&Entry, Error> {
        let mut prefix = Vec::new();

        for component in components {
            match self.entries.get(&Self::canonical(&prefix)) {
                Some(Entry::Dir) => {}
                Some(Entry::File(_)) => {
                    return Err(Error::NotDir(Self::canonical(&prefix)));
                }
                None => {
                    return Err(Error::Missing(Self::canonical(&prefix)));
                }
            }

            prefix.push(component.clone());
            if !self.entries.contains_key(&Self::canonical(&prefix)) {
                return Err(Error::Missing(Self::canonical(&prefix)));
            }
        }

        self.entries
            .get(&Self::canonical(components))
            .ok_or_else(|| Error::Missing(Self::canonical(components)))
    }

    fn parent_components(components: &[String]) -> &[String] {
        &components[..components.len().saturating_sub(1)]
    }

    pub fn mkdir(&mut self, path: &str) -> Result<(), Error> {
        let components = self.normalize(path)?;
        if components.is_empty() {
            return Err(Error::Exists("/".to_owned()));
        }

        let parent = Self::parent_components(&components);
        match self.lookup(parent)? {
            Entry::Dir => {}
            Entry::File(_) => return Err(Error::NotDir(Self::canonical(parent))),
        }

        let canonical = Self::canonical(&components);
        if self.entries.contains_key(&canonical) {
            return Err(Error::Exists(canonical));
        }

        self.entries.insert(canonical, Entry::Dir);
        Ok(())
    }

    pub fn write(&mut self, path: &str, data: &[u8]) -> Result<(), Error> {
        let components = self.normalize(path)?;
        if components.is_empty() {
            return Err(Error::IsDir("/".to_owned()));
        }

        let parent = Self::parent_components(&components);
        match self.lookup(parent)? {
            Entry::Dir => {}
            Entry::File(_) => return Err(Error::NotDir(Self::canonical(parent))),
        }

        let canonical = Self::canonical(&components);
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
        match self.lookup(&components)? {
            Entry::Dir => Err(Error::IsDir(Self::canonical(&components))),
            Entry::File(contents) => Ok(contents.clone()),
        }
    }

    pub fn ls(&self, path: &str) -> Result<Vec<(String, Kind)>, Error> {
        let components = self.normalize(path)?;
        let directory = Self::canonical(&components);
        match self.lookup(&components)? {
            Entry::Dir => {}
            Entry::File(_) => return Err(Error::NotDir(directory)),
        }

        let prefix = if directory == "/" {
            "/".to_owned()
        } else {
            format!("{}/", directory)
        };

        let mut children = Vec::new();
        for (entry_path, entry) in &self.entries {
            if let Some(remainder) = entry_path.strip_prefix(&prefix) {
                if !remainder.is_empty() && !remainder.contains('/') {
                    children.push((remainder.to_owned(), entry.kind()));
                }
            }
        }
        children.sort_by(|left, right| left.0.cmp(&right.0));
        Ok(children)
    }

    pub fn cwd(&self) -> String {
        Self::canonical(&self.cwd)
    }

    pub fn chdir(&mut self, path: &str) -> Result<(), Error> {
        let components = self.normalize(path)?;
        match self.lookup(&components)? {
            Entry::Dir => {
                self.cwd = components;
                Ok(())
            }
            Entry::File(_) => Err(Error::NotDir(Self::canonical(&components))),
        }
    }
}

impl Default for Fs {
    fn default() -> Self {
        Self::new()
    }
}