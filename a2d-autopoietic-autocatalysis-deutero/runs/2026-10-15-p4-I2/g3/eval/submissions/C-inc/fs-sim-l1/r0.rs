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

        let mut components = Vec::new();
        let mut start = 1;
        for (index, byte) in path.bytes().enumerate().skip(1) {
            if byte == b'/' {
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

    fn entry_at<'a>(&'a self, components: &[&str]) -> Result<&'a Entry, Error> {
        let mut entry = &self.root;
        let mut prefix = Vec::new();

        for component in components {
            let children = match entry {
                Entry::Dir(children) => children,
                Entry::File(_) => return Err(Error::NotDir(Self::canonical(&prefix))),
            };

            prefix.push(*component);
            entry = match children.get(*component) {
                Some(entry) => entry,
                None => return Err(Error::Missing(Self::canonical(&prefix))),
            };
        }

        Ok(entry)
    }

    fn parent_mut<'a>(
        &'a mut self,
        components: &[&str],
    ) -> Result<(&'a mut BTreeMap<String, Entry>, String), Error> {
        let (name, parent_components) = components
            .split_last()
            .expect("non-root path has a final component");
        let parent_path = Self::canonical(parent_components);

        let mut entry = &mut self.root;
        let mut prefix = Vec::new();

        for component in parent_components {
            let children = match entry {
                Entry::Dir(children) => children,
                Entry::File(_) => return Err(Error::NotDir(Self::canonical(&prefix))),
            };

            prefix.push(*component);
            entry = match children.get_mut(*component) {
                Some(entry) => entry,
                None => return Err(Error::Missing(Self::canonical(&prefix))),
            };
        }

        match entry {
            Entry::Dir(children) => Ok((children, (*name).to_owned())),
            Entry::File(_) => Err(Error::NotDir(parent_path)),
        }
    }

    pub fn mkdir(&mut self, path: &str) -> Result<(), Error> {
        let components = Self::components(path)?;
        if components.is_empty() {
            return Err(Error::Exists("/".to_owned()));
        }

        let canonical = Self::canonical(&components);
        let (parent, name) = self.parent_mut(&components)?;
        if parent.contains_key(&name) {
            return Err(Error::Exists(canonical));
        }

        parent.insert(name, Entry::Dir(BTreeMap::new()));
        Ok(())
    }

    pub fn write(&mut self, path: &str, data: &[u8]) -> Result<(), Error> {
        let components = Self::components(path)?;
        if components.is_empty() {
            return Err(Error::IsDir("/".to_owned()));
        }

        let canonical = Self::canonical(&components);
        let (parent, name) = self.parent_mut(&components)?;
        match parent.get_mut(&name) {
            Some(Entry::Dir(_)) => Err(Error::IsDir(canonical)),
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
        let components = Self::components(path)?;
        match self.entry_at(&components)? {
            Entry::Dir(_) => Err(Error::IsDir(Self::canonical(&components))),
            Entry::File(contents) => Ok(contents.clone()),
        }
    }

    pub fn ls(&self, path: &str) -> Result<Vec<(String, Kind)>, Error> {
        let components = Self::components(path)?;
        let canonical = Self::canonical(&components);
        match self.entry_at(&components)? {
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
            Entry::File(_) => Err(Error::NotDir(canonical)),
        }
    }
}

fn main() {}