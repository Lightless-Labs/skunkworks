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

#[derive(Debug, Clone, PartialEq, Eq)]
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

    fn parse_path(path: &str) -> Result<Vec<&str>, Error> {
        if let Some(index) = path.as_bytes().iter().position(|&byte| byte == 0) {
            return Err(Error::InvalidPath(index));
        }
        if path.is_empty() || !path.starts_with('/') {
            return Err(Error::InvalidPath(0));
        }
        if path == "/" {
            return Ok(Vec::new());
        }

        let bytes = path.as_bytes();
        let mut components = Vec::new();
        let mut start = 1;
        for (index, byte) in bytes.iter().enumerate().skip(1) {
            if *byte == b'/' {
                if index == start {
                    return Err(Error::InvalidPath(index));
                }
                let component = &path[start..index];
                if component == "." || component == ".." {
                    return Err(Error::InvalidPath(start));
                }
                components.push(component);
                start = index + 1;
            }
        }

        if start == bytes.len() {
            return Err(Error::InvalidPath(bytes.len() - 1));
        }
        let component = &path[start..];
        if component == "." || component == ".." {
            return Err(Error::InvalidPath(start));
        }
        components.push(component);

        Ok(components)
    }

    fn canonical_prefix(components: &[&str]) -> String {
        if components.is_empty() {
            "/".to_string()
        } else {
            let mut path = String::new();
            for component in components {
                path.push('/');
                path.push_str(component);
            }
            path
        }
    }

    fn lookup<'a>(&'a self, components: &[&str]) -> Result<&'a Entry, Error> {
        let mut current = &self.root;
        let mut prefix = Vec::new();

        for component in components {
            match current {
                Entry::Dir(children) => {
                    prefix.push(*component);
                    current = children
                        .get(*component)
                        .ok_or_else(|| Error::Missing(Self::canonical_prefix(&prefix)))?;
                }
                Entry::File(_) => {
                    return Err(Error::NotDir(Self::canonical_prefix(&prefix)));
                }
            }
        }

        Ok(current)
    }

    fn parent_mut<'a>(
        &'a mut self,
        components: &[&str],
    ) -> Result<&'a mut BTreeMap<String, Entry>, Error> {
        let parent_components = &components[..components.len() - 1];
        let mut current = &mut self.root;
        let mut prefix = Vec::new();

        for component in parent_components {
            match current {
                Entry::Dir(children) => {
                    prefix.push(*component);
                    current = children
                        .get_mut(*component)
                        .ok_or_else(|| Error::Missing(Self::canonical_prefix(&prefix)))?;
                }
                Entry::File(_) => {
                    return Err(Error::NotDir(Self::canonical_prefix(&prefix)));
                }
            }
        }

        match current {
            Entry::Dir(children) => Ok(children),
            Entry::File(_) => Err(Error::NotDir(Self::canonical_prefix(parent_components))),
        }
    }

    pub fn mkdir(&mut self, path: &str) -> Result<(), Error> {
        let components = Self::parse_path(path)?;
        if components.is_empty() {
            return Err(Error::Exists("/".to_string()));
        }

        let full_path = Self::canonical_prefix(&components);
        let parent = self.parent_mut(&components)?;
        let name = components[components.len() - 1];

        if parent.contains_key(name) {
            return Err(Error::Exists(full_path));
        }

        parent.insert(name.to_string(), Entry::Dir(BTreeMap::new()));
        Ok(())
    }

    pub fn write(&mut self, path: &str, data: &[u8]) -> Result<(), Error> {
        let components = Self::parse_path(path)?;
        if components.is_empty() {
            return Err(Error::IsDir("/".to_string()));
        }

        let full_path = Self::canonical_prefix(&components);
        let parent = self.parent_mut(&components)?;
        let name = components[components.len() - 1];

        match parent.get_mut(name) {
            Some(Entry::Dir(_)) => Err(Error::IsDir(full_path)),
            Some(Entry::File(contents)) => {
                contents.clear();
                contents.extend_from_slice(data);
                Ok(())
            }
            None => {
                parent.insert(name.to_string(), Entry::File(data.to_vec()));
                Ok(())
            }
        }
    }

    pub fn read(&self, path: &str) -> Result<Vec<u8>, Error> {
        let components = Self::parse_path(path)?;
        match self.lookup(&components)? {
            Entry::Dir(_) => Err(Error::IsDir(Self::canonical_prefix(&components))),
            Entry::File(contents) => Ok(contents.clone()),
        }
    }

    pub fn ls(&self, path: &str) -> Result<Vec<(String, Kind)>, Error> {
        let components = Self::parse_path(path)?;
        let entry = self.lookup(&components)?;
        match entry {
            Entry::File(_) => Err(Error::NotDir(Self::canonical_prefix(&components))),
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