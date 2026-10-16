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

#[derive(Debug, Clone)]
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
        if let Some(index) = path.as_bytes().iter().position(|&b| b == 0) {
            return Err(Error::InvalidPath(index));
        }

        if path.is_empty() || !path.starts_with('/') {
            return Err(Error::InvalidPath(0));
        }

        if path == "/" {
            return Ok(Vec::new());
        }

        let mut components = Vec::new();
        let bytes = path.as_bytes();
        let mut start = 1;

        for index in 1..=bytes.len() {
            if index == bytes.len() || bytes[index] == b'/' {
                let component = &path[start..index];
                if component.is_empty() || component == "." || component == ".." {
                    return Err(Error::InvalidPath(if component.is_empty() {
                        index.saturating_sub(1)
                    } else {
                        start
                    }));
                }
                components.push(component);
                start = index + 1;
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

    fn find<'a>(root: &'a Entry, components: &[&str]) -> Result<&'a Entry, Error> {
        let mut current = root;
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

    fn parent_mut<'a>(
        root: &'a mut Entry,
        parent_components: &[&str],
    ) -> Result<&'a mut BTreeMap<String, Entry>, Error> {
        let mut current = root;
        let mut prefix = Vec::new();

        for component in parent_components {
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

        match current {
            Entry::Dir(children) => Ok(children),
            Entry::File(_) => Err(Error::NotDir(Self::canonical(parent_components))),
        }
    }

    pub fn mkdir(&mut self, path: &str) -> Result<(), Error> {
        let components = Self::validate(path)?;
        if components.is_empty() {
            return Err(Error::Exists("/".to_owned()));
        }

        let (name, parent_components) = components.split_last().expect("nonempty path");
        let parent = Self::parent_mut(&mut self.root, parent_components)?;
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

        let (name, parent_components) = components.split_last().expect("nonempty path");
        let parent = Self::parent_mut(&mut self.root, parent_components)?;

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
        match Self::find(&self.root, &components)? {
            Entry::Dir(_) => Err(Error::IsDir(Self::canonical(&components))),
            Entry::File(contents) => Ok(contents.clone()),
        }
    }

    pub fn ls(&self, path: &str) -> Result<Vec<(String, Kind)>, Error> {
        let components = Self::validate(path)?;
        match Self::find(&self.root, &components)? {
            Entry::File(_) => Err(Error::NotDir(Self::canonical(&components))),
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