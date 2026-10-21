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
        if let Some(i) = path.bytes().position(|b| b == 0) {
            return Err(Error::InvalidPath(i));
        }
        if path.is_empty() || !path.starts_with('/') {
            return Err(Error::InvalidPath(0));
        }
        if path == "/" {
            return Ok(Vec::new());
        }

        let mut components = Vec::new();
        let mut start = 1;
        for (i, byte) in path.bytes().enumerate().skip(1) {
            if byte == b'/' {
                if i == start {
                    return Err(Error::InvalidPath(i));
                }
                let component = &path[start..i];
                if component == "." || component == ".." {
                    return Err(Error::InvalidPath(start));
                }
                components.push(component);
                start = i + 1;
            }
        }

        if start == path.len() {
            return Err(Error::InvalidPath(path.len() - 1));
        }
        let component = &path[start..];
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

    fn entry_at<'a>(root: &'a Entry, components: &[&str]) -> Option<&'a Entry> {
        let mut current = root;
        for component in components {
            match current {
                Entry::Dir(children) => current = children.get(*component)?,
                Entry::File(_) => return None,
            }
        }
        Some(current)
    }

    fn entry_at_mut<'a>(
        root: &'a mut Entry,
        components: &[&str],
    ) -> Option<&'a mut Entry> {
        let mut current = root;
        for component in components {
            match current {
                Entry::Dir(children) => current = children.get_mut(*component)?,
                Entry::File(_) => return None,
            }
        }
        Some(current)
    }

    fn check_parent(&self, components: &[&str]) -> Result<(), Error> {
        let parent_components = &components[..components.len() - 1];
        let mut current = &self.root;
        let mut prefix = Vec::new();

        for component in parent_components {
            let path = Self::canonical(&prefix);
            match current {
                Entry::Dir(children) => match children.get(*component) {
                    Some(entry) => {
                        prefix.push(*component);
                        current = entry;
                    }
                    None => {
                        prefix.push(*component);
                        return Err(Error::Missing(Self::canonical(&prefix)));
                    }
                },
                Entry::File(_) => return Err(Error::NotDir(path)),
            }
        }

        match current {
            Entry::Dir(_) => Ok(()),
            Entry::File(_) => Err(Error::NotDir(Self::canonical(parent_components))),
        }
    }

    fn lookup(&self, components: &[&str]) -> Result<&Entry, Error> {
        let mut current = &self.root;
        let mut prefix = Vec::new();

        for (index, component) in components.iter().enumerate() {
            let current_path = Self::canonical(&prefix);
            match current {
                Entry::Dir(children) => match children.get(*component) {
                    Some(entry) => {
                        prefix.push(*component);
                        current = entry;
                    }
                    None => {
                        prefix.push(*component);
                        return Err(Error::Missing(Self::canonical(&prefix)));
                    }
                },
                Entry::File(_) => return Err(Error::NotDir(current_path)),
            }

            if index + 1 < components.len() && matches!(current, Entry::File(_)) {
                return Err(Error::NotDir(Self::canonical(&prefix)));
            }
        }

        Ok(current)
    }

    pub fn mkdir(&mut self, path: &str) -> Result<(), Error> {
        let components = Self::components(path)?;
        if components.is_empty() {
            return Err(Error::Exists("/".to_owned()));
        }
        self.check_parent(&components)?;

        let parent_components = &components[..components.len() - 1];
        let name = components[components.len() - 1];
        let target = Self::canonical(&components);
        let parent = Self::entry_at_mut(&mut self.root, parent_components)
            .expect("validated directory parent");
        match parent {
            Entry::Dir(children) => {
                if children.contains_key(name) {
                    return Err(Error::Exists(target));
                }
                children.insert(name.to_owned(), Entry::Dir(BTreeMap::new()));
                Ok(())
            }
            Entry::File(_) => Err(Error::NotDir(Self::canonical(parent_components))),
        }
    }

    pub fn write(&mut self, path: &str, data: &[u8]) -> Result<(), Error> {
        let components = Self::components(path)?;
        if components.is_empty() {
            return Err(Error::IsDir("/".to_owned()));
        }
        self.check_parent(&components)?;

        let parent_components = &components[..components.len() - 1];
        let name = components[components.len() - 1];
        let target = Self::canonical(&components);
        let parent = Self::entry_at_mut(&mut self.root, parent_components)
            .expect("validated directory parent");
        match parent {
            Entry::Dir(children) => match children.get_mut(name) {
                Some(Entry::Dir(_)) => Err(Error::IsDir(target)),
                Some(Entry::File(contents)) => {
                    contents.clear();
                    contents.extend_from_slice(data);
                    Ok(())
                }
                None => {
                    children.insert(name.to_owned(), Entry::File(data.to_vec()));
                    Ok(())
                }
            },
            Entry::File(_) => Err(Error::NotDir(Self::canonical(parent_components))),
        }
    }

    pub fn read(&self, path: &str) -> Result<Vec<u8>, Error> {
        let components = Self::components(path)?;
        let target = Self::canonical(&components);
        match self.lookup(&components)? {
            Entry::Dir(_) => Err(Error::IsDir(target)),
            Entry::File(contents) => Ok(contents.clone()),
        }
    }

    pub fn ls(&self, path: &str) -> Result<Vec<(String, Kind)>, Error> {
        let components = Self::components(path)?;
        let target = Self::canonical(&components);
        match self.lookup(&components)? {
            Entry::File(_) => Err(Error::NotDir(target)),
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