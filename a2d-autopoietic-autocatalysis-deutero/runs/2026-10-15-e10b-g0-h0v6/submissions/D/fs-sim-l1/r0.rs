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
        if let Some(i) = path.as_bytes().iter().position(|&b| b == 0) {
            return Err(Error::InvalidPath(i));
        }
        if path.is_empty() || !path.starts_with('/') {
            return Err(Error::InvalidPath(0));
        }
        if path == "/" {
            return Ok(Vec::new());
        }

        let bytes = path.as_bytes();
        let mut result = Vec::new();
        let mut start = 1;

        for i in 1..=bytes.len() {
            if i == bytes.len() || bytes[i] == b'/' {
                let component = &path[start..i];
                if component.is_empty() || component == "." || component == ".." {
                    return Err(Error::InvalidPath(if component.is_empty() {
                        i.saturating_sub(1)
                    } else {
                        start
                    }));
                }
                result.push(component);
                start = i + 1;
            }
        }

        Ok(result)
    }

    fn prefix(components: &[&str]) -> String {
        if components.is_empty() {
            return "/".to_owned();
        }
        let mut path = String::new();
        for component in components {
            path.push('/');
            path.push_str(component);
        }
        path
    }

    pub fn mkdir(&mut self, path: &str) -> Result<(), Error> {
        let components = Self::components(path)?;
        if components.is_empty() {
            return Err(Error::Exists("/".to_owned()));
        }

        let (name, parents) = components.split_last().unwrap();
        let mut entry = &mut self.root;

        for i in 0..parents.len() {
            let prefix = Self::prefix(&parents[..=i]);
            match entry {
                Entry::Dir(children) => {
                    entry = children
                        .get_mut(parents[i])
                        .ok_or_else(|| Error::Missing(prefix.clone()))?;
                }
                Entry::File(_) => return Err(Error::NotDir(prefix)),
            }
        }

        let parent_path = Self::prefix(parents);
        match entry {
            Entry::File(_) => return Err(Error::NotDir(parent_path)),
            Entry::Dir(children) => {
                if children.contains_key(*name) {
                    return Err(Error::Exists(Self::prefix(&components)));
                }
                children.insert((*name).to_owned(), Entry::Dir(BTreeMap::new()));
            }
        }
        Ok(())
    }

    pub fn write(&mut self, path: &str, data: &[u8]) -> Result<(), Error> {
        let components = Self::components(path)?;
        if components.is_empty() {
            return Err(Error::IsDir("/".to_owned()));
        }

        let (name, parents) = components.split_last().unwrap();
        let mut entry = &mut self.root;

        for i in 0..parents.len() {
            let prefix = Self::prefix(&parents[..=i]);
            match entry {
                Entry::Dir(children) => {
                    entry = children
                        .get_mut(parents[i])
                        .ok_or_else(|| Error::Missing(prefix.clone()))?;
                }
                Entry::File(_) => return Err(Error::NotDir(prefix)),
            }
        }

        let parent_path = Self::prefix(parents);
        match entry {
            Entry::File(_) => return Err(Error::NotDir(parent_path)),
            Entry::Dir(children) => match children.get_mut(*name) {
                Some(Entry::Dir(_)) => {
                    return Err(Error::IsDir(Self::prefix(&components)));
                }
                Some(Entry::File(contents)) => contents.clone_from(&data.to_vec()),
                None => {
                    children.insert((*name).to_owned(), Entry::File(data.to_vec()));
                }
            },
        }
        Ok(())
    }

    pub fn read(&self, path: &str) -> Result<Vec<u8>, Error> {
        let components = Self::components(path)?;
        let mut entry = &self.root;

        for (i, component) in components.iter().enumerate() {
            let prefix = Self::prefix(&components[..=i]);
            match entry {
                Entry::Dir(children) => {
                    entry = children
                        .get(*component)
                        .ok_or_else(|| Error::Missing(prefix.clone()))?;
                }
                Entry::File(_) => return Err(Error::NotDir(prefix)),
            }
        }

        match entry {
            Entry::Dir(_) => Err(Error::IsDir(Self::prefix(&components))),
            Entry::File(contents) => Ok(contents.clone()),
        }
    }

    pub fn ls(&self, path: &str) -> Result<Vec<(String, Kind)>, Error> {
        let components = Self::components(path)?;
        let mut entry = &self.root;

        for (i, component) in components.iter().enumerate() {
            let prefix = Self::prefix(&components[..=i]);
            match entry {
                Entry::Dir(children) => {
                    entry = children
                        .get(*component)
                        .ok_or_else(|| Error::Missing(prefix.clone()))?;
                }
                Entry::File(_) => return Err(Error::NotDir(prefix)),
            }
        }

        match entry {
            Entry::File(_) => Err(Error::NotDir(Self::prefix(&components))),
            Entry::Dir(children) => Ok(children
                .iter()
                .map(|(name, child)| {
                    let kind = match child {
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