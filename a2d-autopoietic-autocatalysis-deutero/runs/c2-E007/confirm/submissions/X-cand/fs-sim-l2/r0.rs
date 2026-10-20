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
            components.extend(self.cwd.split('/').filter(|c| !c.is_empty()));
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

    fn parent_path(path: &str) -> &str {
        match path.rfind('/') {
            Some(0) | None => "/",
            Some(i) => &path[..i],
        }
    }

    fn lookup_parent(&self, path: &str) -> Result<(), Error> {
        let parent = Self::parent_path(path);
        let mut current = String::from("/");

        if parent != "/" {
            for component in parent.split('/').filter(|c| !c.is_empty()) {
                if current == "/" {
                    current.push_str(component);
                } else {
                    current.push('/');
                    current.push_str(component);
                }

                match self.entries.get(&current) {
                    None => return Err(Error::Missing(current)),
                    Some(Entry::File(_)) => return Err(Error::NotDir(current)),
                    Some(Entry::Dir) => {}
                }
            }
        }

        Ok(())
    }

    fn lookup_full(&self, path: &str) -> Result<&Entry, Error> {
        if path == "/" {
            return self
                .entries
                .get("/")
                .ok_or_else(|| Error::Missing("/".to_string()));
        }

        let mut current = String::from("/");
        let components: Vec<&str> = path.split('/').filter(|c| !c.is_empty()).collect();

        for (index, component) in components.iter().enumerate() {
            if index > 0 {
                match self.entries.get(&current) {
                    None => return Err(Error::Missing(current)),
                    Some(Entry::File(_)) => return Err(Error::NotDir(current)),
                    Some(Entry::Dir) => {}
                }
                current.push('/');
            }
            current.push_str(component);

            if index + 1 < components.len() {
                match self.entries.get(&current) {
                    None => return Err(Error::Missing(current)),
                    Some(Entry::File(_)) => return Err(Error::NotDir(current)),
                    Some(Entry::Dir) => {}
                }
            }
        }

        self.entries
            .get(&current)
            .ok_or_else(|| Error::Missing(current))
    }

    pub fn mkdir(&mut self, path: &str) -> Result<(), Error> {
        let canonical = self.normalize(path)?;
        self.lookup_parent(&canonical)?;

        if self.entries.contains_key(&canonical) {
            return Err(Error::Exists(canonical));
        }

        self.entries.insert(canonical, Entry::Dir);
        Ok(())
    }

    pub fn write(&mut self, path: &str, data: &[u8]) -> Result<(), Error> {
        let canonical = self.normalize(path)?;
        self.lookup_parent(&canonical)?;

        match self.entries.get(&canonical) {
            Some(Entry::Dir) => return Err(Error::IsDir(canonical)),
            Some(Entry::File(_)) | None => {}
        }

        self.entries
            .insert(canonical, Entry::File(data.to_vec()));
        Ok(())
    }

    pub fn read(&self, path: &str) -> Result<Vec<u8>, Error> {
        let canonical = self.normalize(path)?;
        match self.lookup_full(&canonical)? {
            Entry::File(data) => Ok(data.clone()),
            Entry::Dir => Err(Error::IsDir(canonical)),
        }
    }

    pub fn ls(&self, path: &str) -> Result<Vec<(String, Kind)>, Error> {
        let canonical = self.normalize(path)?;
        match self.lookup_full(&canonical)? {
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
            if let Some(rest) = entry_path.strip_prefix(&prefix) {
                if !rest.is_empty() && !rest.contains('/') {
                    let kind = match entry {
                        Entry::Dir => Kind::Dir,
                        Entry::File(_) => Kind::File,
                    };
                    children.push((rest.to_string(), kind));
                }
            }
        }
        children.sort_by(|a, b| a.0.cmp(&b.0));
        Ok(children)
    }

    pub fn cwd(&self) -> String {
        self.cwd.clone()
    }

    pub fn chdir(&mut self, path: &str) -> Result<(), Error> {
        let canonical = self.normalize(path)?;
        match self.lookup_full(&canonical)? {
            Entry::Dir => {
                self.cwd = canonical;
                Ok(())
            }
            Entry::File(_) => Err(Error::NotDir(canonical)),
        }
    }
}

impl Default for Fs {
    fn default() -> Self {
        Self::new()
    }
}