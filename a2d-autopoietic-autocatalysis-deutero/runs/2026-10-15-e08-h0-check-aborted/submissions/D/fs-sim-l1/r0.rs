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

    pub fn mkdir(&mut self, path: &str) -> Result<(), Error> {
        let components = validate(path)?;
        if components.is_empty() {
            return Err(Error::Exists("/".to_string()));
        }

        let (parents, name) = components.split_at(components.len() - 1);
        let full_path = canonical_path(&components);
        let parent_path = canonical_path(parents);
        let parent = self.lookup_dir_mut(parents)?;

        match parent {
            Entry::Dir(children) => {
                if children.contains_key(name[0]) {
                    return Err(Error::Exists(full_path));
                }
                children.insert(name[0].to_string(), Entry::Dir(BTreeMap::new()));
                Ok(())
            }
            Entry::File(_) => Err(Error::NotDir(parent_path)),
        }
    }

    pub fn write(&mut self, path: &str, data: &[u8]) -> Result<(), Error> {
        let components = validate(path)?;
        if components.is_empty() {
            return Err(Error::IsDir("/".to_string()));
        }

        let (parents, name) = components.split_at(components.len() - 1);
        let full_path = canonical_path(&components);
        let parent_path = canonical_path(parents);
        let parent = self.lookup_dir_mut(parents)?;

        match parent {
            Entry::Dir(children) => match children.get_mut(name[0]) {
                Some(Entry::Dir(_)) => Err(Error::IsDir(full_path)),
                Some(Entry::File(contents)) => {
                    contents.clear();
                    contents.extend_from_slice(data);
                    Ok(())
                }
                None => {
                    children.insert(name[0].to_string(), Entry::File(data.to_vec()));
                    Ok(())
                }
            },
            Entry::File(_) => Err(Error::NotDir(parent_path)),
        }
    }

    pub fn read(&self, path: &str) -> Result<Vec<u8>, Error> {
        let components = validate(path)?;
        if components.is_empty() {
            return Err(Error::IsDir("/".to_string()));
        }

        match self.lookup(&components)? {
            Entry::File(contents) => Ok(contents.clone()),
            Entry::Dir(_) => Err(Error::IsDir(canonical_path(&components))),
        }
    }

    pub fn ls(&self, path: &str) -> Result<Vec<(String, Kind)>, Error> {
        let components = validate(path)?;
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
            Entry::File(_) => Err(Error::NotDir(canonical_path(&components))),
        }
    }

    fn lookup(&self, components: &[&str]) -> Result<&Entry, Error> {
        let mut current = &self.root;
        let mut prefix = Vec::new();

        for component in components {
            if matches!(current, Entry::File(_)) {
                return Err(Error::NotDir(canonical_path(&prefix)));
            }

            prefix.push(*component);
            match current {
                Entry::Dir(children) => match children.get(*component) {
                    Some(entry) => current = entry,
                    None => return Err(Error::Missing(canonical_path(&prefix))),
                },
                Entry::File(_) => unreachable!(),
            }
        }

        Ok(current)
    }

    fn lookup_dir_mut(&mut self, components: &[&str]) -> Result<&mut Entry, Error> {
        let mut current = &mut self.root;
        let mut prefix = Vec::new();

        for component in components {
            if matches!(current, Entry::File(_)) {
                return Err(Error::NotDir(canonical_path(&prefix)));
            }

            prefix.push(*component);
            match current {
                Entry::Dir(children) => match children.get_mut(*component) {
                    Some(entry) => current = entry,
                    None => return Err(Error::Missing(canonical_path(&prefix))),
                },
                Entry::File(_) => unreachable!(),
            }
        }

        Ok(current)
    }
}

impl Default for Fs {
    fn default() -> Self {
        Self::new()
    }
}

fn validate(path: &str) -> Result<Vec<&str>, Error> {
    if let Some(offset) = path.as_bytes().iter().position(|byte| *byte == 0) {
        return Err(Error::InvalidPath(offset));
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

    for (index, byte) in bytes.iter().enumerate().skip(1) {
        if *byte == b'/' {
            let component = &path[start..index];
            if component.is_empty() || component == "." || component == ".." {
                return Err(Error::InvalidPath(if component.is_empty() {
                    index
                } else {
                    start
                }));
            }
            components.push(component);
            start = index + 1;
        }
    }

    let component = &path[start..];
    if component.is_empty() || component == "." || component == ".." {
        return Err(Error::InvalidPath(if component.is_empty() {
            bytes.len() - 1
        } else {
            start
        }));
    }
    components.push(component);

    Ok(components)
}

fn canonical_path(components: &[&str]) -> String {
    if components.is_empty() {
        return "/".to_string();
    }

    let mut path = String::new();
    for component in components {
        path.push('/');
        path.push_str(component);
    }
    path
}