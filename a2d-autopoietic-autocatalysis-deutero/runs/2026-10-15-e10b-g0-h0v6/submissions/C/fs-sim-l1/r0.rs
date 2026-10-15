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
    Dir(std::collections::BTreeMap<String, Entry>),
    File(Vec<u8>),
}

pub struct Fs {
    root: Entry,
}

impl Fs {
    pub fn new() -> Self {
        Self {
            root: Entry::Dir(std::collections::BTreeMap::new()),
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

        let bytes = path.as_bytes();
        let mut components = Vec::new();
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
            return "/".to_string();
        }
        let mut path = String::new();
        for component in components {
            path.push('/');
            path.push_str(component);
        }
        path
    }

    fn lookup<'a>(
        &'a self,
        components: &[&str],
    ) -> Result<&'a Entry, Error> {
        let mut current = &self.root;
        let mut prefix = Vec::new();

        for component in components {
            match current {
                Entry::Dir(children) => {
                    prefix.push(*component);
                    let canonical = Self::canonical(&prefix);
                    current = children
                        .get(*component)
                        .ok_or(Error::Missing(canonical))?;
                }
                Entry::File(_) => {
                    return Err(Error::NotDir(Self::canonical(&prefix)));
                }
            }
        }
        Ok(current)
    }

    fn lookup_mut<'a>(
        &'a mut self,
        components: &[&str],
    ) -> Result<&'a mut Entry, Error> {
        let mut current = &mut self.root;
        let mut prefix = Vec::new();

        for component in components {
            match current {
                Entry::Dir(children) => {
                    prefix.push(*component);
                    let canonical = Self::canonical(&prefix);
                    current = children
                        .get_mut(*component)
                        .ok_or(Error::Missing(canonical))?;
                }
                Entry::File(_) => {
                    return Err(Error::NotDir(Self::canonical(&prefix)));
                }
            }
        }
        Ok(current)
    }

    pub fn mkdir(&mut self, path: &str) -> Result<(), Error> {
        let components = Self::validate(path)?;
        if components.is_empty() {
            return Err(Error::Exists("/".to_string()));
        }

        let parent_components = &components[..components.len() - 1];
        let name = components[components.len() - 1];
        let parent_path = Self::canonical(parent_components);
        let entry_path = Self::canonical(&components);

        let parent = self.lookup_mut(parent_components)?;
        match parent {
            Entry::Dir(children) => {
                if children.contains_key(name) {
                    return Err(Error::Exists(entry_path));
                }
                children.insert(
                    name.to_string(),
                    Entry::Dir(std::collections::BTreeMap::new()),
                );
                Ok(())
            }
            Entry::File(_) => Err(Error::NotDir(parent_path)),
        }
    }

    pub fn write(&mut self, path: &str, data: &[u8]) -> Result<(), Error> {
        let components = Self::validate(path)?;
        if components.is_empty() {
            return Err(Error::IsDir("/".to_string()));
        }

        let parent_components = &components[..components.len() - 1];
        let name = components[components.len() - 1];
        let parent_path = Self::canonical(parent_components);
        let entry_path = Self::canonical(&components);

        let parent = self.lookup_mut(parent_components)?;
        match parent {
            Entry::Dir(children) => match children.get_mut(name) {
                Some(Entry::Dir(_)) => Err(Error::IsDir(entry_path)),
                Some(Entry::File(contents)) => {
                    *contents = data.to_vec();
                    Ok(())
                }
                None => {
                    children.insert(name.to_string(), Entry::File(data.to_vec()));
                    Ok(())
                }
            },
            Entry::File(_) => Err(Error::NotDir(parent_path)),
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

fn main() {}