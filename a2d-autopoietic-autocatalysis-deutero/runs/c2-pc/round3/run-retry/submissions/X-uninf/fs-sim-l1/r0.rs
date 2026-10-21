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
                    let offset = if component.is_empty() && i == bytes.len() {
                        bytes.len() - 1
                    } else if component.is_empty() {
                        i
                    } else {
                        start
                    };
                    return Err(Error::InvalidPath(offset));
                }
                result.push(component);
                start = i + 1;
            }
        }
        Ok(result)
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

    fn lookup_mut<'a>(&'a mut self, components: &[&str]) -> Result<&'a mut Entry, Error> {
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
        Ok(entry)
    }

    pub fn mkdir(&mut self, path: &str) -> Result<(), Error> {
        let components = Self::components(path)?;
        if components.is_empty() {
            return Err(Error::Exists("/".to_owned()));
        }

        let (parent_components, final_name) = components.split_at(components.len() - 1);
        let parent_path = Self::canonical(parent_components);
        let final_path = Self::canonical(&components);

        let parent = self.lookup_mut(parent_components)?;
        match parent {
            Entry::Dir(children) => {
                if children.contains_key(final_name[0]) {
                    Err(Error::Exists(final_path))
                } else {
                    children.insert(final_name[0].to_owned(), Entry::Dir(BTreeMap::new()));
                    Ok(())
                }
            }
            Entry::File(_) => Err(Error::NotDir(parent_path)),
        }
    }

    pub fn write(&mut self, path: &str, data: &[u8]) -> Result<(), Error> {
        let components = Self::components(path)?;
        if components.is_empty() {
            return Err(Error::IsDir("/".to_owned()));
        }

        let (parent_components, final_name) = components.split_at(components.len() - 1);
        let parent_path = Self::canonical(parent_components);
        let final_path = Self::canonical(&components);

        let parent = self.lookup_mut(parent_components)?;
        match parent {
            Entry::Dir(children) => match children.get_mut(final_name[0]) {
                Some(Entry::Dir(_)) => Err(Error::IsDir(final_path)),
                Some(Entry::File(contents)) => {
                    contents.clear();
                    contents.extend_from_slice(data);
                    Ok(())
                }
                None => {
                    children.insert(final_name[0].to_owned(), Entry::File(data.to_vec()));
                    Ok(())
                }
            },
            Entry::File(_) => Err(Error::NotDir(parent_path)),
        }
    }

    pub fn read(&self, path: &str) -> Result<Vec<u8>, Error> {
        let components = Self::components(path)?;
        match self.lookup(&components)? {
            Entry::File(contents) => Ok(contents.clone()),
            Entry::Dir(_) => Err(Error::IsDir(Self::canonical(&components))),
        }
    }

    pub fn ls(&self, path: &str) -> Result<Vec<(String, Kind)>, Error> {
        let components = Self::components(path)?;
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