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

        Ok(components)
    }

    fn prefix(components: &[&str]) -> String {
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

    fn entry_at<'a>(root: &'a Entry, components: &[&str]) -> Result<&'a Entry, Error> {
        let mut current = root;
        let mut traversed = Vec::new();

        for (index, component) in components.iter().enumerate() {
            match current {
                Entry::Dir(children) => {
                    traversed.push(*component);
                    current = children
                        .get(*component)
                        .ok_or_else(|| Error::Missing(Self::prefix(&traversed)))?;
                }
                Entry::File(_) => {
                    return Err(Error::NotDir(Self::prefix(&traversed)));
                }
            }

            if index + 1 < components.len() && matches!(current, Entry::File(_)) {
                return Err(Error::NotDir(Self::prefix(&traversed)));
            }
        }

        Ok(current)
    }

    fn parent_mut<'a>(
        root: &'a mut Entry,
        components: &[&str],
    ) -> Result<&'a mut BTreeMap<String, Entry>, Error> {
        let (name, parents) = components.split_last().expect("non-root path");
        let _ = name;
        let mut current = root;
        let mut traversed = Vec::new();

        for component in parents {
            match current {
                Entry::Dir(children) => {
                    traversed.push(*component);
                    current = children
                        .get_mut(*component)
                        .ok_or_else(|| Error::Missing(Self::prefix(&traversed)))?;
                }
                Entry::File(_) => return Err(Error::NotDir(Self::prefix(&traversed))),
            }
            if matches!(current, Entry::File(_)) {
                return Err(Error::NotDir(Self::prefix(&traversed)));
            }
        }

        match current {
            Entry::Dir(children) => Ok(children),
            Entry::File(_) => Err(Error::NotDir(Self::prefix(parents))),
        }
    }

    pub fn mkdir(&mut self, path: &str) -> Result<(), Error> {
        let components = Self::components(path)?;
        if components.is_empty() {
            return Err(Error::Exists("/".to_owned()));
        }

        let name = *components.last().unwrap();
        let parent = Self::parent_mut(&mut self.root, &components)?;
        if parent.contains_key(name) {
            return Err(Error::Exists(Self::prefix(&components)));
        }

        parent.insert(name.to_owned(), Entry::Dir(BTreeMap::new()));
        Ok(())
    }

    pub fn write(&mut self, path: &str, data: &[u8]) -> Result<(), Error> {
        let components = Self::components(path)?;
        if components.is_empty() {
            return Err(Error::IsDir("/".to_owned()));
        }

        let name = *components.last().unwrap();
        let full_path = Self::prefix(&components);
        let parent = Self::parent_mut(&mut self.root, &components)?;

        match parent.get_mut(name) {
            Some(Entry::Dir(_)) => Err(Error::IsDir(full_path)),
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
        let components = Self::components(path)?;
        match Self::entry_at(&self.root, &components)? {
            Entry::File(contents) => Ok(contents.clone()),
            Entry::Dir(_) => Err(Error::IsDir(Self::prefix(&components))),
        }
    }

    pub fn ls(&self, path: &str) -> Result<Vec<(String, Kind)>, Error> {
        let components = Self::components(path)?;
        match Self::entry_at(&self.root, &components)? {
            Entry::File(_) => Err(Error::NotDir(Self::prefix(&components))),
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