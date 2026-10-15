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
    Dir,
    File(Vec<u8>),
}

pub struct Fs {
    entries: BTreeMap<String, Entry>,
    cwd: String,
}

impl Fs {
    pub fn new() -> Self {
        let mut entries = BTreeMap::new();
        entries.insert("/".to_owned(), Entry::Dir);
        Self {
            entries,
            cwd: "/".to_owned(),
        }
    }

    fn normalize(&self, path: &str) -> Result<String, Error> {
        if let Some(index) = path.as_bytes().iter().position(|&byte| byte == 0) {
            return Err(Error::InvalidPath(index));
        }
        if path.is_empty() {
            return Err(Error::InvalidPath(0));
        }

        let mut components: Vec<&str> = Vec::new();
        if !path.starts_with('/') {
            components.extend(self.cwd.split('/').filter(|part| !part.is_empty()));
        }

        for component in path.split('/') {
            match component {
                "" | "." => {}
                ".." => {
                    components.pop();
                }
                other => components.push(other),
            }
        }

        if components.is_empty() {
            Ok("/".to_owned())
        } else {
            let mut result = String::new();
            for component in components {
                result.push('/');
                result.push_str(component);
            }
            Ok(result)
        }
    }

    fn parent_and_name(path: &str) -> Option<(&str, &str)> {
        if path == "/" {
            return None;
        }
        match path.rfind('/') {
            Some(0) => Some(("/", &path[1..])),
            Some(index) => Some((&path[..index], &path[index + 1..])),
            None => None,
        }
    }

    fn lookup(&self, path: &str) -> Result<&Entry, Error> {
        if path == "/" {
            return self
                .entries
                .get("/")
                .ok_or_else(|| Error::Missing("/".to_owned()));
        }

        let mut prefix = String::new();
        for component in path.split('/').filter(|part| !part.is_empty()) {
            prefix.push('/');
            prefix.push_str(component);

            let entry = self
                .entries
                .get(&prefix)
                .ok_or_else(|| Error::Missing(prefix.clone()))?;

            if prefix != path && !matches!(entry, Entry::Dir) {
                return Err(Error::NotDir(prefix));
            }
        }

        self.entries
            .get(path)
            .ok_or_else(|| Error::Missing(path.to_owned()))
    }

    pub fn mkdir(&mut self, path: &str) -> Result<(), Error> {
        let canonical = self.normalize(path)?;
        let (parent, _) = Self::parent_and_name(&canonical)
            .ok_or_else(|| Error::Exists(canonical.clone()))?;

        match self.lookup(parent)? {
            Entry::Dir => {}
            Entry::File(_) => return Err(Error::NotDir(parent.to_owned())),
        }

        if self.entries.contains_key(&canonical) {
            return Err(Error::Exists(canonical));
        }

        self.entries.insert(canonical, Entry::Dir);
        Ok(())
    }

    pub fn write(&mut self, path: &str, data: &[u8]) -> Result<(), Error> {
        let canonical = self.normalize(path)?;
        if canonical == "/" {
            return Err(Error::IsDir(canonical));
        }

        let (parent, _) = Self::parent_and_name(&canonical).expect("non-root path has parent");
        match self.lookup(parent)? {
            Entry::Dir => {}
            Entry::File(_) => return Err(Error::NotDir(parent.to_owned())),
        }

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
        let canonical = self.normalize(path)?;
        match self.lookup(&canonical)? {
            Entry::Dir => Err(Error::IsDir(canonical)),
            Entry::File(contents) => Ok(contents.clone()),
        }
    }

    pub fn ls(&self, path: &str) -> Result<Vec<(String, Kind)>, Error> {
        let canonical = self.normalize(path)?;
        match self.lookup(&canonical)? {
            Entry::Dir => {}
            Entry::File(_) => return Err(Error::NotDir(canonical)),
        }

        let prefix = if canonical == "/" {
            "/".to_owned()
        } else {
            format!("{canonical}/")
        };

        let mut children = Vec::new();
        for (entry_path, entry) in &self.entries {
            if let Some(remainder) = entry_path.strip_prefix(&prefix) {
                if !remainder.is_empty() && !remainder.contains('/') {
                    let kind = match entry {
                        Entry::Dir => Kind::Dir,
                        Entry::File(_) => Kind::File,
                    };
                    children.push((remainder.to_owned(), kind));
                }
            }
        }
        children.sort_by(|left, right| left.0.cmp(&right.0));
        Ok(children)
    }

    pub fn cwd(&self) -> String {
        self.cwd.clone()
    }

    pub fn chdir(&mut self, path: &str) -> Result<(), Error> {
        let canonical = self.normalize(path)?;
        match self.lookup(&canonical)? {
            Entry::Dir => {
                self.cwd = canonical;
                Ok(())
            }
            Entry::File(_) => Err(Error::NotDir(canonical)),
        }
    }
}

fn main() {}