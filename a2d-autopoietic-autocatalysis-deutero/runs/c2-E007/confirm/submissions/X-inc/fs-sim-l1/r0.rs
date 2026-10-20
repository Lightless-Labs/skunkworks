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

    fn validate(path: &str) -> Result<Vec<&str>, Error> {
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

        for index in 1..=bytes.len() {
            if index == bytes.len() || bytes[index] == b'/' {
                let component = &path[start..index];
                if component.is_empty() || component == "." || component == ".." {
                    let offset = if component.is_empty() {
                        if index == bytes.len() {
                            bytes.len() - 1
                        } else {
                            index
                        }
                    } else {
                        start
                    };
                    return Err(Error::InvalidPath(offset));
                }
                components.push(component);
                start = index + 1;
            }
        }

        Ok(components)
    }

    fn canonical(components: &[&str]) -> String {
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

    fn child_path(parent: &[&str], child: &str) -> String {
        let mut components = parent.to_vec();
        components.push(child);
        Self::canonical(&components)
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
                Entry::File(_) => {
                    return Err(Error::NotDir(Self::canonical(&prefix)));
                }
            }
        }

        Ok(current)
    }

    fn parent_dir_mut<'a>(
        &'a mut self,
        components: &[&str],
    ) -> Result<&'a mut BTreeMap<String, Entry>, Error> {
        let mut current = &mut self.root;
        let mut prefix = Vec::new();

        for component in components {
            match current {
                Entry::Dir(children) => {
                    prefix.push(*component);
                    current = children
                        .get_mut(*component)
                        .ok_or_else(|| Error::Missing(Self::canonical(&prefix)))?;
                }
                Entry::File(_) => {
                    return Err(Error::NotDir(Self::canonical(&prefix)));
                }
            }
        }

        match current {
            Entry::Dir(children) => Ok(children),
            Entry::File(_) => Err(Error::NotDir(Self::canonical(components))),
        }
    }

    pub fn mkdir(&mut self, path: &str) -> Result<(), Error> {
        let components = Self::validate(path)?;
        if components.is_empty() {
            return Err(Error::Exists("/".to_string()));
        }

        let (name, parent) = components.split_last().unwrap();
        let parent_dir = self.parent_dir_mut(parent)?;
        if parent_dir.contains_key(*name) {
            return Err(Error::Exists(Self::canonical(&components)));
        }
        parent_dir.insert((*name).to_string(), Entry::Dir(BTreeMap::new()));
        Ok(())
    }

    pub fn write(&mut self, path: &str, data: &[u8]) -> Result<(), Error> {
        let components = Self::validate(path)?;
        if components.is_empty() {
            return Err(Error::IsDir("/".to_string()));
        }

        let (name, parent) = components.split_last().unwrap();
        let parent_dir = self.parent_dir_mut(parent)?;
        match parent_dir.get_mut(*name) {
            Some(Entry::Dir(_)) => Err(Error::IsDir(Self::canonical(&components))),
            Some(Entry::File(contents)) => {
                *contents = data.to_vec();
                Ok(())
            }
            None => {
                parent_dir.insert((*name).to_string(), Entry::File(data.to_vec()));
                Ok(())
            }
        }
    }

    pub fn read(&self, path: &str) -> Result<Vec<u8>, Error> {
        let components = Self::validate(path)?;
        match self.lookup(&components)? {
            Entry::Dir(_) => Err(Error::IsDir(Self::canonical(&components))),
            Entry::File(contents) => Ok(contents.clone()),
        }
    }

    pub fn ls(&self, path: &str) -> Result<Vec<(String, Kind)>, Error> {
        let components = Self::validate(path)?;
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