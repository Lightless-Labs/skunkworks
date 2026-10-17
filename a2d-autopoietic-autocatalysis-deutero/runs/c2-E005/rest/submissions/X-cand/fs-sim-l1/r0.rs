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

    fn validate_path(path: &str) -> Result<Vec<&str>, Error> {
        if let Some(index) = path.as_bytes().iter().position(|&byte| byte == 0) {
            return Err(Error::InvalidPath(index));
        }

        if path.is_empty() || !path.starts_with('/') {
            return Err(Error::InvalidPath(0));
        }

        if path == "/" {
            return Ok(Vec::new());
        }

        let mut components = Vec::new();
        let mut component_start = 1;

        for (index, byte) in path.bytes().enumerate().skip(1) {
            if byte == b'/' {
                let component = &path[component_start..index];
                if component.is_empty() || component == "." || component == ".." {
                    return Err(Error::InvalidPath(if component.is_empty() {
                        index
                    } else {
                        component_start
                    }));
                }
                components.push(component);
                component_start = index + 1;
            }
        }

        let component = &path[component_start..];
        if component.is_empty() || component == "." || component == ".." {
            return Err(Error::InvalidPath(if component.is_empty() {
                path.len() - 1
            } else {
                component_start
            }));
        }
        components.push(component);

        Ok(components)
    }

    fn prefix(components: &[&str]) -> String {
        if components.is_empty() {
            "/".to_owned()
        } else {
            format!("/{}", components.join("/"))
        }
    }

    fn lookup<'a>(&'a self, components: &[&str]) -> Result<&'a Entry, Error> {
        let mut entry = &self.root;
        let mut prefix = Vec::new();

        for (index, component) in components.iter().enumerate() {
            match entry {
                Entry::Dir(children) => {
                    prefix.push(*component);
                    entry = children
                        .get(*component)
                        .ok_or_else(|| Error::Missing(Self::prefix(&prefix)))?;
                }
                Entry::File(_) => return Err(Error::NotDir(Self::prefix(&prefix))),
            }

            if index + 1 < components.len() && matches!(entry, Entry::File(_)) {
                return Err(Error::NotDir(Self::prefix(&prefix)));
            }
        }

        Ok(entry)
    }

    fn parent_mut<'a>(
        &'a mut self,
        components: &[&str],
    ) -> Result<&'a mut BTreeMap<String, Entry>, Error> {
        let parent_components = &components[..components.len() - 1];
        let mut entry = &mut self.root;
        let mut prefix = Vec::new();

        for component in parent_components {
            match entry {
                Entry::Dir(children) => {
                    prefix.push(*component);
                    entry = children
                        .get_mut(*component)
                        .ok_or_else(|| Error::Missing(Self::prefix(&prefix)))?;
                }
                Entry::File(_) => return Err(Error::NotDir(Self::prefix(&prefix))),
            }
        }

        match entry {
            Entry::Dir(children) => Ok(children),
            Entry::File(_) => Err(Error::NotDir(Self::prefix(parent_components))),
        }
    }

    pub fn mkdir(&mut self, path: &str) -> Result<(), Error> {
        let components = Self::validate_path(path)?;
        if components.is_empty() {
            return Err(Error::Exists("/".to_owned()));
        }

        let canonical = Self::prefix(&components);
        let name = components[components.len() - 1];
        let parent = self.parent_mut(&components)?;

        if parent.contains_key(name) {
            return Err(Error::Exists(canonical));
        }

        parent.insert(name.to_owned(), Entry::Dir(BTreeMap::new()));
        Ok(())
    }

    pub fn write(&mut self, path: &str, data: &[u8]) -> Result<(), Error> {
        let components = Self::validate_path(path)?;
        if components.is_empty() {
            return Err(Error::IsDir("/".to_owned()));
        }

        let canonical = Self::prefix(&components);
        let name = components[components.len() - 1];
        let parent = self.parent_mut(&components)?;

        match parent.get_mut(name) {
            Some(Entry::Dir(_)) => Err(Error::IsDir(canonical)),
            Some(Entry::File(contents)) => {
                contents.clear();
                contents.extend_from_slice(data);
                Ok(())
            }
            None => {
                parent.insert(name.to_owned(), Entry::File(data.to_vec()));
                Ok(())
            }
        }
    }

    pub fn read(&self, path: &str) -> Result<Vec<u8>, Error> {
        let components = Self::validate_path(path)?;
        let canonical = Self::prefix(&components);

        match self.lookup(&components)? {
            Entry::Dir(_) => Err(Error::IsDir(canonical)),
            Entry::File(contents) => Ok(contents.clone()),
        }
    }

    pub fn ls(&self, path: &str) -> Result<Vec<(String, Kind)>, Error> {
        let components = Self::validate_path(path)?;
        let canonical = Self::prefix(&components);

        match self.lookup(&components)? {
            Entry::File(_) => Err(Error::NotDir(canonical)),
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