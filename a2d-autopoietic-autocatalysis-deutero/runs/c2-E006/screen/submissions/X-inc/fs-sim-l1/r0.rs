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

    fn validate(path: &str) -> Result<Vec<&str>, Error> {
        if let Some(offset) = path.as_bytes().iter().position(|&byte| byte == 0) {
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
        for (index, byte) in path.as_bytes().iter().enumerate().skip(1) {
            if *byte == b'/' {
                let component = &path[start..index];
                if component.is_empty() {
                    return Err(Error::InvalidPath(index));
                }
                if component == "." || component == ".." {
                    return Err(Error::InvalidPath(start));
                }
                components.push(component);
                start = index + 1;
            }
        }

        let component = &path[start..];
        if component.is_empty() {
            return Err(Error::InvalidPath(path.len() - 1));
        }
        if component == "." || component == ".." {
            return Err(Error::InvalidPath(start));
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

    fn lookup<'a>(&'a self, components: &[&str]) -> Result<&'a Entry, Error> {
        let mut entry = &self.root;
        let mut prefix = Vec::new();

        for component in components {
            match entry {
                Entry::Dir(children) => {
                    prefix.push(*component);
                    entry = children
                        .get(*component)
                        .ok_or_else(|| Error::Missing(Self::canonical(&prefix)))?;
                }
                Entry::File(_) => return Err(Error::NotDir(Self::canonical(&prefix))),
            }
        }

        Ok(entry)
    }

    fn parent_dir_mut<'a>(
        &'a mut self,
        components: &[&str],
    ) -> Result<&'a mut BTreeMap<String, Entry>, Error> {
        let mut entry = &mut self.root;
        let mut prefix = Vec::new();

        for component in components {
            match entry {
                Entry::Dir(children) => {
                    prefix.push(*component);
                    entry = children
                        .get_mut(*component)
                        .ok_or_else(|| Error::Missing(Self::canonical(&prefix)))?;
                }
                Entry::File(_) => return Err(Error::NotDir(Self::canonical(&prefix))),
            }
        }

        match entry {
            Entry::Dir(children) => Ok(children),
            Entry::File(_) => Err(Error::NotDir(Self::canonical(components))),
        }
    }

    pub fn mkdir(&mut self, path: &str) -> Result<(), Error> {
        let components = Self::validate(path)?;
        if components.is_empty() {
            return Err(Error::Exists("/".to_owned()));
        }

        let (name, parents) = components.split_last().expect("non-root path");
        let parent = self.parent_dir_mut(parents)?;
        if parent.contains_key(*name) {
            return Err(Error::Exists(Self::canonical(&components)));
        }
        parent.insert((*name).to_owned(), Entry::Dir(BTreeMap::new()));
        Ok(())
    }

    pub fn write(&mut self, path: &str, data: &[u8]) -> Result<(), Error> {
        let components = Self::validate(path)?;
        if components.is_empty() {
            return Err(Error::IsDir("/".to_owned()));
        }

        let (name, parents) = components.split_last().expect("non-root path");
        let parent = self.parent_dir_mut(parents)?;
        match parent.get_mut(*name) {
            Some(Entry::Dir(_)) => Err(Error::IsDir(Self::canonical(&components))),
            Some(Entry::File(contents)) => {
                contents.clear();
                contents.extend_from_slice(data);
                Ok(())
            }
            None => {
                parent.insert((*name).to_owned(), Entry::File(data.to_vec()));
                Ok(())
            }
        }
    }

    pub fn read(&self, path: &str) -> Result<Vec<u8>, Error> {
        let components = Self::validate(path)?;
        match self.lookup(&components)? {
            Entry::File(contents) => Ok(contents.clone()),
            Entry::Dir(_) => Err(Error::IsDir(Self::canonical(&components))),
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

impl Default for Fs {
    fn default() -> Self {
        Self::new()
    }
}