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

    fn validate(path: &str) -> Result<Vec<&str>, Error> {
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

        let mut components = Vec::new();
        let mut start = 1;
        for (i, b) in path.bytes().enumerate().skip(1) {
            if b == b'/' {
                let component = &path[start..i];
                if component.is_empty() || component == "." || component == ".." {
                    return Err(Error::InvalidPath(if component.is_empty() {
                        i
                    } else {
                        start
                    }));
                }
                components.push(component);
                start = i + 1;
            }
        }

        let component = &path[start..];
        if component.is_empty() || component == "." || component == ".." {
            return Err(Error::InvalidPath(if component.is_empty() {
                path.len() - 1
            } else {
                start
            }));
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

        for (i, component) in components.iter().enumerate() {
            match entry {
                Entry::Dir(children) => {
                    prefix.push(*component);
                    entry = children.get(*component).ok_or_else(|| {
                        Error::Missing(Self::canonical(&prefix))
                    })?;
                    if i + 1 < components.len() && matches!(entry, Entry::File(_)) {
                        return Err(Error::NotDir(Self::canonical(&prefix)));
                    }
                }
                Entry::File(_) => {
                    return Err(Error::NotDir(Self::canonical(&prefix)));
                }
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
                    entry = children.get_mut(*component).ok_or_else(|| {
                        Error::Missing(Self::canonical(&prefix))
                    })?;
                    if matches!(entry, Entry::File(_)) {
                        return Err(Error::NotDir(Self::canonical(&prefix)));
                    }
                }
                Entry::File(_) => {
                    return Err(Error::NotDir(Self::canonical(&prefix)));
                }
            }
        }

        match entry {
            Entry::Dir(children) => Ok(children),
            Entry::File(_) => Err(Error::NotDir(Self::canonical(parent_components))),
        }
    }

    pub fn mkdir(&mut self, path: &str) -> Result<(), Error> {
        let components = Self::validate(path)?;
        if components.is_empty() {
            return Err(Error::Exists("/".to_owned()));
        }

        let final_path = Self::canonical(&components);
        let name = components[components.len() - 1].to_owned();
        let parent = self.parent_mut(&components)?;
        if parent.contains_key(&name) {
            return Err(Error::Exists(final_path));
        }

        parent.insert(name, Entry::Dir(BTreeMap::new()));
        Ok(())
    }

    pub fn write(&mut self, path: &str, data: &[u8]) -> Result<(), Error> {
        let components = Self::validate(path)?;
        if components.is_empty() {
            return Err(Error::IsDir("/".to_owned()));
        }

        let final_path = Self::canonical(&components);
        let name = components[components.len() - 1].to_owned();
        let parent = self.parent_mut(&components)?;
        match parent.get_mut(&name) {
            Some(Entry::Dir(_)) => Err(Error::IsDir(final_path)),
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
        let components = Self::validate(path)?;
        match self.lookup(&components)? {
            Entry::Dir(_) => Err(Error::IsDir(Self::canonical(&components))),
            Entry::File(contents) => Ok(contents.clone()),
        }
    }

    pub fn ls(&self, path: &str) -> Result<Vec<(String, Kind)>, Error> {
        let components = Self::validate(path)?;
        let entry = self.lookup(&components)?;
        match entry {
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