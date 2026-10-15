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
    Dir,
    File(Vec<u8>),
}

pub struct Fs {
    entries: std::collections::HashMap<String, Entry>,
    cwd: String,
}

impl Fs {
    pub fn new() -> Self {
        let mut entries = std::collections::HashMap::new();
        entries.insert("/".to_string(), Entry::Dir);
        Self {
            entries,
            cwd: "/".to_string(),
        }
    }

    fn normalize(&self, path: &str) -> Result<String, Error> {
        if let Some(i) = path.as_bytes().iter().position(|&b| b == 0) {
            return Err(Error::InvalidPath(i));
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
            Ok("/".to_string())
        } else {
            Ok(format!("/{}", components.join("/")))
        }
    }

    fn parent_and_name(path: &str) -> Option<(String, &str)> {
        if path == "/" {
            return None;
        }
        let index = path.rfind('/').expect("canonical path has a slash");
        let name = &path[index + 1..];
        let parent = if index == 0 { "/" } else { &path[..index] };
        Some((parent.to_string(), name))
    }

    fn check_parent(&self, path: &str) -> Result<(), Error> {
        let (parent, _) = Self::parent_and_name(path).expect("root has no parent");
        match self.entries.get(&parent) {
            None => Err(Error::Missing(parent)),
            Some(Entry::Dir) => Ok(()),
            Some(Entry::File(_)) => Err(Error::NotDir(parent)),
        }
    }

    fn lookup(&self, path: &str) -> Result<&Entry, Error> {
        if path == "/" {
            return self
                .entries
                .get("/")
                .ok_or_else(|| Error::Missing("/".to_string()));
        }

        let mut prefix = String::new();
        let components: Vec<&str> = path.split('/').filter(|part| !part.is_empty()).collect();
        for (i, component) in components.iter().enumerate() {
            prefix.push('/');
            prefix.push_str(component);
            let entry = self
                .entries
                .get(&prefix)
                .ok_or_else(|| Error::Missing(prefix.clone()))?;
            if i + 1 < components.len() && !matches!(entry, Entry::Dir) {
                return Err(Error::NotDir(prefix));
            }
        }
        self.entries
            .get(path)
            .ok_or_else(|| Error::Missing(path.to_string()))
    }

    pub fn mkdir(&mut self, path: &str) -> Result<(), Error> {
        let canonical = self.normalize(path)?;
        if canonical == "/" {
            return Err(Error::Exists(canonical));
        }
        self.check_parent(&canonical)?;
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
        self.check_parent(&canonical)?;

        match self.entries.get(&canonical) {
            Some(Entry::Dir) => Err(Error::IsDir(canonical)),
            Some(Entry::File(_)) => {
                self.entries
                    .insert(canonical, Entry::File(data.to_vec()));
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
            Entry::File(data) => Ok(data.clone()),
        }
    }

    pub fn ls(&self, path: &str) -> Result<Vec<(String, Kind)>, Error> {
        let canonical = self.normalize(path)?;
        match self.lookup(&canonical)? {
            Entry::File(_) => return Err(Error::NotDir(canonical)),
            Entry::Dir => {}
        }

        let prefix = if canonical == "/" {
            "/".to_string()
        } else {
            format!("{}/", canonical)
        };
        let mut children = Vec::new();

        for (entry_path, entry) in &self.entries {
            if !entry_path.starts_with(&prefix) {
                continue;
            }
            let remainder = &entry_path[prefix.len()..];
            if remainder.is_empty() || remainder.contains('/') {
                continue;
            }
            let kind = match entry {
                Entry::Dir => Kind::Dir,
                Entry::File(_) => Kind::File,
            };
            children.push((remainder.to_string(), kind));
        }

        children.sort_by(|a, b| a.0.cmp(&b.0));
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