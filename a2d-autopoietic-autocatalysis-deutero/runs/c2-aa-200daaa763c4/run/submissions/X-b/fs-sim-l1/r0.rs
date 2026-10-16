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
        if let Some(i) = path.as_bytes().iter().position(|&b| b == 0) {
            return Err(Error::InvalidPath(i));
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
            format!("/{}", components.join("/"))
        }
    }

    fn find<'a>(root: &'a Entry, components: &[&str]) -> Result<&'a Entry, Error> {
        let mut current = root;

        for (i, component) in components.iter().enumerate() {
            let prefix = Self::canonical(&components[..=i]);
            match current {
                Entry::Dir(children) => {
                    current = children
                        .get(*component)
                        .ok_or_else(|| Error::Missing(prefix))?;
                }
                Entry::File(_) => return Err(Error::NotDir(Self::canonical(&components[..i]))),
            }
        }

        Ok(current)
    }

    fn find_mut<'a>(
        root: &'a mut Entry,
        components: &[&str],
    ) -> Result<&'a mut Entry, Error> {
        let mut current = root;

        for (i, component) in components.iter().enumerate() {
            let prefix = Self::canonical(&components[..=i]);
            match current {
                Entry::Dir(children) => {
                    current = children
                        .get_mut(*component)
                        .ok_or_else(|| Error::Missing(prefix))?;
                }
                Entry::File(_) => return Err(Error::NotDir(Self::canonical(&components[..i]))),
            }
        }

        Ok(current)
    }

    pub fn mkdir(&mut self, path: &str) -> Result<(), Error> {
        let components = Self::validate(path)?;
        if components.is_empty() {
            return Err(Error::Exists("/".to_owned()));
        }

        let parent_components = &components[..components.len() - 1];
        let name = components[components.len() - 1];
        let parent_path = Self::canonical(parent_components);
        let final_path = Self::canonical(&components);

        let parent = Self::find_mut(&mut self.root, parent_components)?;
        match parent {
            Entry::Dir(children) => {
                if children.contains_key(name) {
                    return Err(Error::Exists(final_path));
                }
                children.insert(name.to_owned(), Entry::Dir(BTreeMap::new()));
                Ok(())
            }
            Entry::File(_) => Err(Error::NotDir(parent_path)),
        }
    }

    pub fn write(&mut self, path: &str, data: &[u8]) -> Result<(), Error> {
        let components = Self::validate(path)?;
        if components.is_empty() {
            return Err(Error::IsDir("/".to_owned()));
        }

        let parent_components = &components[..components.len() - 1];
        let name = components[components.len() - 1];
        let parent_path = Self::canonical(parent_components);
        let final_path = Self::canonical(&components);

        let parent = Self::find_mut(&mut self.root, parent_components)?;
        match parent {
            Entry::Dir(children) => match children.get_mut(name) {
                Some(Entry::Dir(_)) => Err(Error::IsDir(final_path)),
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
            Entry::File(_) => Err(Error::NotDir(parent_path)),
        }
    }

    pub fn read(&self, path: &str) -> Result<Vec<u8>, Error> {
        let components = Self::validate(path)?;
        let final_path = Self::canonical(&components);
        match Self::find(&self.root, &components)? {
            Entry::File(contents) => Ok(contents.clone()),
            Entry::Dir(_) => Err(Error::IsDir(final_path)),
        }
    }

    pub fn ls(&self, path: &str) -> Result<Vec<(String, Kind)>, Error> {
        let components = Self::validate(path)?;
        let final_path = Self::canonical(&components);
        match Self::find(&self.root, &components)? {
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
            Entry::File(_) => Err(Error::NotDir(final_path)),
        }
    }
}