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

#[derive(Debug)]
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
        if let Some(i) = path.as_bytes().iter().position(|&b| b == 0) {
            return Err(Error::InvalidPath(i));
        }
        if path.is_empty() {
            return Err(Error::InvalidPath(0));
        }
        if !path.starts_with('/') {
            return Err(Error::InvalidPath(0));
        }
        if path == "/" {
            return Ok(Vec::new());
        }

        let bytes = path.as_bytes();
        let mut components = Vec::new();
        let mut start = 1;

        for i in 1..=bytes.len() {
            if i == bytes.len() || bytes[i] == b'/' {
                let component = &path[start..i];
                if component.is_empty() {
                    return Err(Error::InvalidPath(i.saturating_sub(1)));
                }
                if component == "." || component == ".." {
                    return Err(Error::InvalidPath(start));
                }
                components.push(component);
                start = i + 1;
            }
        }

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

    fn lookup<'a>(&'a self, components: &[&str]) -> Result<&'a Entry, Error> {
        let mut current = &self.root;
        let mut prefix = Vec::new();

        for component in components {
            match current {
                Entry::Dir(children) => {
                    prefix.push(*component);
                    current = children
                        .get(*component)
                        .ok_or_else(|| Error::Missing(Self::canonical(&prefix)))?;
                }
                Entry::File(_) => return Err(Error::NotDir(Self::canonical(&prefix))),
            }
        }

        Ok(current)
    }

    fn lookup_parent_mut<'a>(
        &'a mut self,
        components: &[&str],
    ) -> Result<(&'a mut BTreeMap<String, Entry>, String), Error> {
        let (name, parents) = components
            .split_last()
            .expect("a non-root path has a final component");

        let mut current = &mut self.root;
        let mut prefix = Vec::new();

        for component in parents {
            match current {
                Entry::Dir(children) => {
                    prefix.push(*component);
                    current = children
                        .get_mut(*component)
                        .ok_or_else(|| Error::Missing(Self::canonical(&prefix)))?;
                }
                Entry::File(_) => return Err(Error::NotDir(Self::canonical(&prefix))),
            }
        }

        let parent_path = Self::canonical(parents);
        match current {
            Entry::Dir(children) => Ok((children, (*name).to_owned())),
            Entry::File(_) => Err(Error::NotDir(parent_path)),
        }
    }

    pub fn mkdir(&mut self, path: &str) -> Result<(), Error> {
        let components = Self::parse_path(path)?;
        if components.is_empty() {
            return Err(Error::Exists("/".to_owned()));
        }

        let full_path = Self::canonical(&components);
        let (parent, name) = self.lookup_parent_mut(&components)?;
        if parent.contains_key(&name) {
            return Err(Error::Exists(full_path));
        }
        parent.insert(name, Entry::Dir(BTreeMap::new()));
        Ok(())
    }

    pub fn write(&mut self, path: &str, data: &[u8]) -> Result<(), Error> {
        let components = Self::parse_path(path)?;
        if components.is_empty() {
            return Err(Error::IsDir("/".to_owned()));
        }

        let full_path = Self::canonical(&components);
        let (parent, name) = self.lookup_parent_mut(&components)?;
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
        let components = Self::parse_path(path)?;
        match self.lookup(&components)? {
            Entry::File(contents) => Ok(contents.clone()),
            Entry::Dir(_) => Err(Error::IsDir(Self::canonical(&components))),
        }
    }

    pub fn ls(&self, path: &str) -> Result<Vec<(String, Kind)>, Error> {
        let components = Self::parse_path(path)?;
        match self.lookup(&components)? {
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
            Entry::File(_) => Err(Error::NotDir(Self::canonical(&components))),
        }
    }
}

impl Default for Fs {
    fn default() -> Self {
        Self::new()
    }
}