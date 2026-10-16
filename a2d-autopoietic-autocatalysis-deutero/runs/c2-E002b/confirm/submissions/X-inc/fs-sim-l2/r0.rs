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
    Dir(BTreeMap<String, Entry>),
    File(Vec<u8>),
}

pub struct Fs {
    root: Entry,
    cwd: Vec<String>,
}

impl Fs {
    pub fn new() -> Self {
        Self {
            root: Entry::Dir(BTreeMap::new()),
            cwd: Vec::new(),
        }
    }

    fn canonical(parts: &[String]) -> String {
        if parts.is_empty() {
            "/".to_owned()
        } else {
            format!("/{}", parts.join("/"))
        }
    }

    fn normalize(&self, path: &str) -> Result<Vec<String>, Error> {
        if let Some(i) = path.as_bytes().iter().position(|&b| b == 0) {
            return Err(Error::InvalidPath(i));
        }
        if path.is_empty() {
            return Err(Error::InvalidPath(0));
        }

        let mut parts = if path.starts_with('/') {
            Vec::new()
        } else {
            self.cwd.clone()
        };

        for component in path.split('/') {
            match component {
                "" | "." => {}
                ".." => {
                    parts.pop();
                }
                name => parts.push(name.to_owned()),
            }
        }
        Ok(parts)
    }

    fn lookup<'a>(&'a self, parts: &[String]) -> Result<&'a Entry, Error> {
        let mut current = &self.root;
        for (i, component) in parts.iter().enumerate() {
            let parent_path = Self::canonical(&parts[..i]);
            let children = match current {
                Entry::Dir(children) => children,
                Entry::File(_) => return Err(Error::NotDir(parent_path)),
            };
            current = children
                .get(component)
                .ok_or_else(|| Error::Missing(Self::canonical(&parts[..=i])))?;
        }
        Ok(current)
    }

    fn lookup_parent_mut<'a>(
        &'a mut self,
        parts: &[String],
    ) -> Result<&'a mut BTreeMap<String, Entry>, Error> {
        let mut current = match &mut self.root {
            Entry::Dir(children) => children,
            Entry::File(_) => unreachable!(),
        };

        for (i, component) in parts[..parts.len() - 1].iter().enumerate() {
            let prefix = Self::canonical(&parts[..=i]);
            let entry = current
                .get_mut(component)
                .ok_or_else(|| Error::Missing(prefix.clone()))?;
            match entry {
                Entry::Dir(children) => current = children,
                Entry::File(_) => return Err(Error::NotDir(prefix)),
            }
        }
        Ok(current)
    }

    pub fn mkdir(&mut self, path: &str) -> Result<(), Error> {
        let parts = self.normalize(path)?;
        let full_path = Self::canonical(&parts);
        if parts.is_empty() {
            return Err(Error::Exists(full_path));
        }

        let parent = self.lookup_parent_mut(&parts)?;
        let name = parts.last().expect("nonempty path");
        if parent.contains_key(name) {
            return Err(Error::Exists(full_path));
        }
        parent.insert(name.clone(), Entry::Dir(BTreeMap::new()));
        Ok(())
    }

    pub fn write(&mut self, path: &str, data: &[u8]) -> Result<(), Error> {
        let parts = self.normalize(path)?;
        let full_path = Self::canonical(&parts);
        if parts.is_empty() {
            return Err(Error::IsDir(full_path));
        }

        let parent = self.lookup_parent_mut(&parts)?;
        let name = parts.last().expect("nonempty path");
        match parent.get_mut(name) {
            Some(Entry::Dir(_)) => Err(Error::IsDir(full_path)),
            Some(Entry::File(contents)) => {
                contents.clear();
                contents.extend_from_slice(data);
                Ok(())
            }
            None => {
                parent.insert(name.clone(), Entry::File(data.to_vec()));
                Ok(())
            }
        }
    }

    pub fn read(&self, path: &str) -> Result<Vec<u8>, Error> {
        let parts = self.normalize(path)?;
        match self.lookup(&parts)? {
            Entry::File(contents) => Ok(contents.clone()),
            Entry::Dir(_) => Err(Error::IsDir(Self::canonical(&parts))),
        }
    }

    pub fn ls(&self, path: &str) -> Result<Vec<(String, Kind)>, Error> {
        let parts = self.normalize(path)?;
        match self.lookup(&parts)? {
            Entry::Dir(children) => Ok(children
                .iter()
                .map(|(name, entry)| {
                    let kind = match entry {
                        Entry::Dir(_) => Kind::Dir,
                        Entry::File(_) => Kind::File,
                    };
                    (name.clone(), kind)
                })
                .collect()),
            Entry::File(_) => Err(Error::NotDir(Self::canonical(&parts))),
        }
    }

    pub fn cwd(&self) -> String {
        Self::canonical(&self.cwd)
    }

    pub fn chdir(&mut self, path: &str) -> Result<(), Error> {
        let parts = self.normalize(path)?;
        match self.lookup(&parts)? {
            Entry::Dir(_) => {
                self.cwd = parts;
                Ok(())
            }
            Entry::File(_) => Err(Error::NotDir(Self::canonical(&parts))),
        }
    }
}