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
    Dir(BTreeMap<String, Entry>),
    File(Vec<u8>),
}

pub struct Fs {
    root: Entry,
}

impl Fs {
    pub fn new() -> Self {
        Self {
            root: Entry::Dir(BTreeMap::new()),
        }
    }

    fn components(path: &str) -> Result<Vec<&str>, Error> {
        if let Some(i) = path.bytes().position(|b| b == 0) {
            return Err(Error::InvalidPath(i));
        }
        if path.is_empty() || !path.starts_with('/') {
            return Err(Error::InvalidPath(0));
        }
        if path == "/" {
            return Ok(Vec::new());
        }

        let mut parts = Vec::new();
        let mut start = 1;
        for (i, byte) in path.bytes().enumerate().skip(1) {
            if byte == b'/' {
                let component = &path[start..i];
                if component.is_empty() || component == "." || component == ".." {
                    return Err(Error::InvalidPath(if component.is_empty() {
                        i
                    } else {
                        start
                    }));
                }
                parts.push(component);
                start = i + 1;
            }
        }

        let component = &path[start..];
        if component.is_empty() || component == "." || component == ".." {
            return Err(Error::InvalidPath(if component.is_empty() {
                path.len() - 1
            } else {
                start
            }));
        }
        parts.push(component);
        Ok(parts)
    }

    fn canonical(parts: &[&str]) -> String {
        if parts.is_empty() {
            "/".to_owned()
        } else {
            let mut path = String::new();
            for part in parts {
                path.push('/');
                path.push_str(part);
            }
            path
        }
    }

    fn lookup<'a>(&'a self, parts: &[&str]) -> Result<&'a Entry, Error> {
        let mut current = &self.root;
        let mut prefix: Vec<&str> = Vec::new();

        for part in parts {
            match current {
                Entry::Dir(children) => {
                    prefix.push(part);
                    current = children
                        .get(*part)
                        .ok_or_else(|| Error::Missing(Self::canonical(&prefix)))?;
                }
                Entry::File(_) => return Err(Error::NotDir(Self::canonical(&prefix))),
            }
        }
        Ok(current)
    }

    fn lookup_parent_mut<'a>(
        &'a mut self,
        parts: &[&str],
    ) -> Result<&'a mut BTreeMap<String, Entry>, Error> {
        let mut current = &mut self.root;
        let mut prefix: Vec<&str> = Vec::new();

        for part in parts {
            match current {
                Entry::Dir(children) => {
                    prefix.push(part);
                    current = children
                        .get_mut(*part)
                        .ok_or_else(|| Error::Missing(Self::canonical(&prefix)))?;
                }
                Entry::File(_) => return Err(Error::NotDir(Self::canonical(&prefix))),
            }
        }

        match current {
            Entry::Dir(children) => Ok(children),
            Entry::File(_) => Err(Error::NotDir(Self::canonical(parts))),
        }
    }

    pub fn mkdir(&mut self, path: &str) -> Result<(), Error> {
        let parts = Self::components(path)?;
        if parts.is_empty() {
            return Err(Error::Exists("/".to_owned()));
        }

        let parent_parts = &parts[..parts.len() - 1];
        let name = parts[parts.len() - 1];
        let children = self.lookup_parent_mut(parent_parts)?;
        let mut full_parts = parent_parts.to_vec();
        full_parts.push(name);
        let full_path = Self::canonical(&full_parts);

        if children.contains_key(name) {
            return Err(Error::Exists(full_path));
        }
        children.insert(name.to_owned(), Entry::Dir(BTreeMap::new()));
        Ok(())
    }

    pub fn write(&mut self, path: &str, data: &[u8]) -> Result<(), Error> {
        let parts = Self::components(path)?;
        if parts.is_empty() {
            return Err(Error::IsDir("/".to_owned()));
        }

        let parent_parts = &parts[..parts.len() - 1];
        let name = parts[parts.len() - 1];
        let children = self.lookup_parent_mut(parent_parts)?;
        let mut full_parts = parent_parts.to_vec();
        full_parts.push(name);
        let full_path = Self::canonical(&full_parts);

        match children.get_mut(name) {
            Some(Entry::Dir(_)) => Err(Error::IsDir(full_path)),
            Some(Entry::File(contents)) => {
                *contents = data.to_vec();
                Ok(())
            }
            None => {
                children.insert(name.to_owned(), Entry::File(data.to_vec()));
                Ok(())
            }
        }
    }

    pub fn read(&self, path: &str) -> Result<Vec<u8>, Error> {
        let parts = Self::components(path)?;
        let entry = self.lookup(&parts)?;
        match entry {
            Entry::Dir(_) => Err(Error::IsDir(Self::canonical(&parts))),
            Entry::File(contents) => Ok(contents.clone()),
        }
    }

    pub fn ls(&self, path: &str) -> Result<Vec<(String, Kind)>, Error> {
        let parts = Self::components(path)?;
        let entry = self.lookup(&parts)?;
        match entry {
            Entry::File(_) => Err(Error::NotDir(Self::canonical(&parts))),
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
        }
    }
}

impl Default for Fs {
    fn default() -> Self {
        Self::new()
    }
}