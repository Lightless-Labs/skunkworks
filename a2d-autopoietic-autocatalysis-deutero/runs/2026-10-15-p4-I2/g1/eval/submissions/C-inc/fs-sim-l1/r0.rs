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
        let mut start = 1;
        for (i, byte) in path.bytes().enumerate().skip(1) {
            if byte == b'/' {
                let component = &path[start..i];
                if component.is_empty() || component == "." || component == ".." {
                    return Err(Error::InvalidPath(if component.is_empty() { i } else { start }));
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

    fn child_mut<'a>(entry: &'a mut Entry, name: &str, path: &str) -> Result<&'a mut Entry, Error> {
        match entry {
            Entry::Dir(children) => children
                .get_mut(name)
                .ok_or_else(|| Error::Missing(path.to_owned())),
            Entry::File(_) => Err(Error::NotDir(path.to_owned())),
        }
    }

    fn child<'a>(entry: &'a Entry, name: &str, path: &str) -> Result<&'a Entry, Error> {
        match entry {
            Entry::Dir(children) => children
                .get(name)
                .ok_or_else(|| Error::Missing(path.to_owned())),
            Entry::File(_) => Err(Error::NotDir(path.to_owned())),
        }
    }

    fn resolve<'a>(&'a self, components: &[&str]) -> Result<&'a Entry, Error> {
        let mut current = &self.root;
        let mut prefix = Vec::new();
        for component in components {
            prefix.push(*component);
            current = Self::child(current, component, &Self::canonical(&prefix))?;
        }
        Ok(current)
    }

    fn resolve_parent_mut<'a>(
        &'a mut self,
        components: &[&str],
    ) -> Result<(&'a mut Entry, String), Error> {
        let parent_components = &components[..components.len() - 1];
        let mut current = &mut self.root;
        let mut prefix = Vec::new();

        for component in parent_components {
            prefix.push(*component);
            current = Self::child_mut(current, component, &Self::canonical(&prefix))?;
        }

        let parent_path = Self::canonical(parent_components);
        if !matches!(current, Entry::Dir(_)) {
            return Err(Error::NotDir(parent_path));
        }
        Ok((current, parent_path))
    }

    pub fn mkdir(&mut self, path: &str) -> Result<(), Error> {
        let components = Self::validate(path)?;
        if components.is_empty() {
            return Err(Error::Exists("/".to_owned()));
        }

        let (parent, _) = self.resolve_parent_mut(&components)?;
        let name = components[components.len() - 1];
        let full_path = Self::canonical(&components);
        match parent {
            Entry::Dir(children) => {
                if children.contains_key(name) {
                    return Err(Error::Exists(full_path));
                }
                children.insert(name.to_owned(), Entry::Dir(BTreeMap::new()));
                Ok(())
            }
            Entry::File(_) => unreachable!(),
        }
    }

    pub fn write(&mut self, path: &str, data: &[u8]) -> Result<(), Error> {
        let components = Self::validate(path)?;
        if components.is_empty() {
            return Err(Error::IsDir("/".to_owned()));
        }

        let (parent, _) = self.resolve_parent_mut(&components)?;
        let name = components[components.len() - 1];
        let full_path = Self::canonical(&components);
        match parent {
            Entry::Dir(children) => match children.get_mut(name) {
                Some(Entry::Dir(_)) => Err(Error::IsDir(full_path)),
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
            Entry::File(_) => unreachable!(),
        }
    }

    pub fn read(&self, path: &str) -> Result<Vec<u8>, Error> {
        let components = Self::validate(path)?;
        let full_path = Self::canonical(&components);
        match self.resolve(&components)? {
            Entry::Dir(_) => Err(Error::IsDir(full_path)),
            Entry::File(contents) => Ok(contents.clone()),
        }
    }

    pub fn ls(&self, path: &str) -> Result<Vec<(String, Kind)>, Error> {
        let components = Self::validate(path)?;
        let full_path = Self::canonical(&components);
        match self.resolve(&components)? {
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
            Entry::File(_) => Err(Error::NotDir(full_path)),
        }
    }
}

impl Default for Fs {
    fn default() -> Self {
        Self::new()
    }
}

fn main() {}