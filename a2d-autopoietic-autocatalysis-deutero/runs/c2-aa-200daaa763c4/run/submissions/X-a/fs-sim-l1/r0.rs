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

    fn validate_path(path: &str) -> Result<Vec<&str>, Error> {
        if let Some(offset) = path.as_bytes().iter().position(|&b| b == 0) {
            return Err(Error::InvalidPath(offset));
        }

        if path.is_empty() || !path.starts_with('/') {
            return Err(Error::InvalidPath(0));
        }

        if path == "/" {
            return Ok(Vec::new());
        }

        let mut components = Vec::new();
        let mut start = 1;
        for (index, byte) in path.bytes().enumerate().skip(1) {
            if byte == b'/' {
                let component = &path[start..index];
                if component.is_empty() || component == "." || component == ".." {
                    return Err(Error::InvalidPath(if component.is_empty() {
                        index
                    } else {
                        start
                    }));
                }
                components.push(component);
                start = index + 1;
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
        components.push(component);
        Ok(components)
    }

    fn canonical(components: &[&str]) -> String {
        if components.is_empty() {
            "/".to_owned()
        } else {
            let mut path = String::new();
            for component in components {
                path.push('/');
                path.push_str(component);
            }
            path
        }
    }

    fn entry_at<'a>(&'a self, components: &[&str]) -> Result<&'a Entry, Error> {
        let mut current = &self.root;
        let mut prefix = Vec::new();

        for component in components {
            match current {
                Entry::Dir(children) => {
                    prefix.push(*component);
                    let canonical = Self::canonical(&prefix);
                    current = children
                        .get(*component)
                        .ok_or(Error::Missing(canonical))?;
                }
                Entry::File(_) => {
                    return Err(Error::NotDir(Self::canonical(&prefix)));
                }
            }
        }
        Ok(current)
    }

    fn parent_mut<'a>(
        &'a mut self,
        components: &[&str],
    ) -> Result<(&'a mut BTreeMap<String, Entry>, String), Error> {
        let (name, parents) = components
            .split_last()
            .expect("validated non-root path has a final component");

        let mut current = &mut self.root;
        let mut prefix = Vec::new();

        for component in parents {
            match current {
                Entry::Dir(children) => {
                    prefix.push(*component);
                    let canonical = Self::canonical(&prefix);
                    current = children
                        .get_mut(*component)
                        .ok_or(Error::Missing(canonical))?;
                }
                Entry::File(_) => {
                    return Err(Error::NotDir(Self::canonical(&prefix)));
                }
            }
        }

        match current {
            Entry::Dir(children) => Ok((children, (*name).to_owned())),
            Entry::File(_) => Err(Error::NotDir(Self::canonical(parents))),
        }
    }

    pub fn mkdir(&mut self, path: &str) -> Result<(), Error> {
        let components = Self::validate_path(path)?;
        if components.is_empty() {
            return Err(Error::Exists("/".to_owned()));
        }

        let full_path = Self::canonical(&components);
        let (parent, name) = self.parent_mut(&components)?;
        if parent.contains_key(&name) {
            return Err(Error::Exists(full_path));
        }
        parent.insert(name, Entry::Dir(BTreeMap::new()));
        Ok(())
    }

    pub fn write(&mut self, path: &str, data: &[u8]) -> Result<(), Error> {
        let components = Self::validate_path(path)?;
        if components.is_empty() {
            return Err(Error::IsDir("/".to_owned()));
        }

        let full_path = Self::canonical(&components);
        let (parent, name) = self.parent_mut(&components)?;
        match parent.get_mut(&name) {
            Some(Entry::Dir(_)) => Err(Error::IsDir(full_path)),
            Some(Entry::File(contents)) => {
                contents.clear();
                contents.extend_from_slice(data);
                Ok(())
            }
            None => {
                parent.insert(name, Entry::File(data.to_vec()));
                Ok(())
            }
        }
    }

    pub fn read(&self, path: &str) -> Result<Vec<u8>, Error> {
        let components = Self::validate_path(path)?;
        let full_path = Self::canonical(&components);
        match self.entry_at(&components)? {
            Entry::Dir(_) => Err(Error::IsDir(full_path)),
            Entry::File(contents) => Ok(contents.clone()),
        }
    }

    pub fn ls(&self, path: &str) -> Result<Vec<(String, Kind)>, Error> {
        let components = Self::validate_path(path)?;
        let full_path = Self::canonical(&components);
        match self.entry_at(&components)? {
            Entry::File(_) => Err(Error::NotDir(full_path)),
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