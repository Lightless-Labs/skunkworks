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

#[derive(Debug)]
enum Entry {
    Dir,
    File(Vec<u8>),
}

pub struct Fs {
    entries: BTreeMap<String, Entry>,
}

impl Fs {
    pub fn new() -> Self {
        let mut entries = BTreeMap::new();
        entries.insert("/".to_string(), Entry::Dir);
        Self { entries }
    }

    fn canonicalize(path: &str) -> Result<Vec<String>, Error> {
        if let Some(index) = path.bytes().position(|byte| byte == 0) {
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
        let bytes = path.as_bytes();

        for end in 1..=bytes.len() {
            if end == bytes.len() || bytes[end] == b'/' {
                if end == start {
                    return Err(Error::InvalidPath(if end == bytes.len() {
                        bytes.len() - 1
                    } else {
                        end
                    }));
                }

                let component = &path[start..end];
                if component == "." || component == ".." {
                    return Err(Error::InvalidPath(start));
                }
                components.push(component.to_string());
                start = end + 1;
            }
        }

        Ok(components)
    }

    fn prefix(components: &[String]) -> String {
        if components.is_empty() {
            "/".to_string()
        } else {
            format!("/{}", components.join("/"))
        }
    }

    fn lookup(&self, components: &[String]) -> Result<&Entry, Error> {
        let mut prefix = Vec::new();
        let mut current = self
            .entries
            .get("/")
            .expect("root directory must always exist");

        if components.is_empty() {
            return Ok(current);
        }

        for (index, component) in components.iter().enumerate() {
            if !matches!(current, Entry::Dir) {
                return Err(Error::NotDir(Self::prefix(&prefix)));
            }
            prefix.push(component.clone());
            current = self
                .entries
                .get(&Self::prefix(&prefix))
                .ok_or_else(|| Error::Missing(Self::prefix(&prefix)))?;

            if index + 1 < components.len() && !matches!(current, Entry::Dir) {
                return Err(Error::NotDir(Self::prefix(&prefix)));
            }
        }

        Ok(current)
    }

    pub fn mkdir(&mut self, path: &str) -> Result<(), Error> {
        let components = Self::canonicalize(path)?;
        if components.is_empty() {
            return Err(Error::Exists("/".to_string()));
        }

        let parent_components = &components[..components.len() - 1];
        self.lookup(parent_components)?;
        let parent_path = Self::prefix(parent_components);
        if !matches!(self.entries.get(&parent_path), Some(Entry::Dir)) {
            return Err(Error::NotDir(parent_path));
        }

        let canonical = Self::prefix(&components);
        if self.entries.contains_key(&canonical) {
            return Err(Error::Exists(canonical));
        }

        self.entries.insert(canonical, Entry::Dir);
        Ok(())
    }

    pub fn write(&mut self, path: &str, data: &[u8]) -> Result<(), Error> {
        let components = Self::canonicalize(path)?;
        if components.is_empty() {
            return Err(Error::IsDir("/".to_string()));
        }

        let parent_components = &components[..components.len() - 1];
        self.lookup(parent_components)?;
        let parent_path = Self::prefix(parent_components);
        if !matches!(self.entries.get(&parent_path), Some(Entry::Dir)) {
            return Err(Error::NotDir(parent_path));
        }

        let canonical = Self::prefix(&components);
        match self.entries.get_mut(&canonical) {
            Some(Entry::Dir) => Err(Error::IsDir(canonical)),
            Some(Entry::File(contents)) => {
                contents.clear();
                contents.extend_from_slice(data);
                Ok(())
            }
            None => {
                self.entries
                    .insert(canonical, Entry::File(data.to_vec()));
                Ok(())
            }
        }
    }

    pub fn read(&self, path: &str) -> Result<Vec<u8>, Error> {
        let components = Self::canonicalize(path)?;
        match self.lookup(&components)? {
            Entry::Dir => Err(Error::IsDir(Self::prefix(&components))),
            Entry::File(contents) => Ok(contents.clone()),
        }
    }

    pub fn ls(&self, path: &str) -> Result<Vec<(String, Kind)>, Error> {
        let components = Self::canonicalize(path)?;
        let canonical = Self::prefix(&components);
        match self.lookup(&components)? {
            Entry::File(_) => return Err(Error::NotDir(canonical.clone())),
            Entry::Dir => {}
        }

        let child_prefix = if canonical == "/" {
            "/".to_string()
        } else {
            format!("{}/", canonical)
        };
        let mut children = Vec::new();

        for (entry_path, entry) in &self.entries {
            if entry_path == &canonical || !entry_path.starts_with(&child_prefix) {
                continue;
            }
            let remainder = &entry_path[child_prefix.len()..];
            if remainder.contains('/') || remainder.is_empty() {
                continue;
            }
            let kind = match entry {
                Entry::Dir => Kind::Dir,
                Entry::File(_) => Kind::File,
            };
            children.push((remainder.to_string(), kind));
        }

        children.sort_by(|left, right| left.0.cmp(&right.0));
        Ok(children)
    }
}