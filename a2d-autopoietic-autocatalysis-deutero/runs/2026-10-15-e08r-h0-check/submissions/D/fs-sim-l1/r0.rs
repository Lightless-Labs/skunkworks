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
        let components = validate_path(path)?;
        if components.is_empty() {
            return Err(Error::Exists("/".to_string()));
        }

        let (parents, name) = components.split_at(components.len() - 1);
        let parent_path = canonical_path(parents);
        let target_path = canonical_path(&components);

        let parent = self.lookup_dir_mut(parents, &parent_path)?;
        if parent.contains_key(name[0]) {
            return Err(Error::Exists(target_path));
        }
        parent.insert(name[0].to_string(), Entry::Dir(BTreeMap::new()));
        Ok(())
    }

    pub fn write(&mut self, path: &str, data: &[u8]) -> Result<(), Error> {
        let components = validate_path(path)?;
        if components.is_empty() {
            return Err(Error::IsDir("/".to_string()));
        }

        let (parents, name) = components.split_at(components.len() - 1);
        let parent_path = canonical_path(parents);
        let target_path = canonical_path(&components);

        let parent = self.lookup_dir_mut(parents, &parent_path)?;
        match parent.get_mut(name[0]) {
            Some(Entry::Dir(_)) => Err(Error::IsDir(target_path)),
            Some(Entry::File(contents)) => {
                contents.clear();
                contents.extend_from_slice(data);
                Ok(())
            }
            None => {
                parent.insert(name[0].to_string(), Entry::File(data.to_vec()));
                Ok(())
            }
        }
    }

    pub fn read(&self, path: &str) -> Result<Vec<u8>, Error> {
        let components = validate_path(path)?;
        if components.is_empty() {
            return Err(Error::IsDir("/".to_string()));
        }

        let target_path = canonical_path(&components);
        match self.lookup_entry(&components, &target_path)? {
            Entry::File(contents) => Ok(contents.clone()),
            Entry::Dir(_) => Err(Error::IsDir(target_path)),
        }
    }

    pub fn ls(&self, path: &str) -> Result<Vec<(String, Kind)>, Error> {
        let components = validate_path(path)?;
        let target_path = canonical_path(&components);
        let entry = self.lookup_entry(&components, &target_path)?;

        match entry {
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
            Entry::File(_) => Err(Error::NotDir(target_path)),
        }
    }

    fn lookup_entry<'a>(
        &'a self,
        components: &[&str],
        target_path: &str,
    ) -> Result<&'a Entry, Error> {
        if components.is_empty() {
            return Ok(&self.root);
        }

        let mut current = &self.root;
        for (index, component) in components.iter().enumerate() {
            let prefix = canonical_path(&components[..=index]);
            let children = match current {
                Entry::Dir(children) => children,
                Entry::File(_) => {
                    return Err(Error::NotDir(canonical_path(&components[..index])));
                }
            };

            current = children
                .get(*component)
                .ok_or_else(|| Error::Missing(prefix))?;
        }

        let _ = target_path;
        Ok(current)
    }

    fn lookup_dir_mut<'a>(
        &'a mut self,
        components: &[&str],
        parent_path: &str,
    ) -> Result<&'a mut BTreeMap<String, Entry>, Error> {
        let mut current = &mut self.root;

        for (index, component) in components.iter().enumerate() {
            let prefix = canonical_path(&components[..=index]);
            match current {
                Entry::Dir(children) => {
                    current = children
                        .get_mut(*component)
                        .ok_or_else(|| Error::Missing(prefix))?;
                }
                Entry::File(_) => {
                    return Err(Error::NotDir(canonical_path(&components[..index])));
                }
            }
        }

        match current {
            Entry::Dir(children) => Ok(children),
            Entry::File(_) => Err(Error::NotDir(parent_path.to_string())),
        }
    }
}

impl Default for Fs {
    fn default() -> Self {
        Self::new()
    }
}

fn validate_path(path: &str) -> Result<Vec<&str>, Error> {
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

    for (index, byte) in bytes.iter().enumerate().skip(1) {
        if *byte == b'/' {
            if index == start {
                return Err(Error::InvalidPath(index));
            }
            let component = &path[start..index];
            if component == "." || component == ".." {
                return Err(Error::InvalidPath(start));
            }
            components.push(component);
            start = index + 1;
        }
    }

    if start == bytes.len() {
        return Err(Error::InvalidPath(bytes.len() - 1));
    }

    let component = &path[start..];
    if component == "." || component == ".." {
        return Err(Error::InvalidPath(start));
    }
    components.push(component);

    Ok(components)
}

fn canonical_path(components: &[&str]) -> String {
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