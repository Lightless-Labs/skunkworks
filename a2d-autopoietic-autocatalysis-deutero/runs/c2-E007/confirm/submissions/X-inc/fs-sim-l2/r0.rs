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

    fn parent_path(path: &str) -> &str {
        match path.rfind('/') {
            Some(0) | None => "/",
            Some(index) => &path[..index],
        }
    }

    fn basename(path: &str) -> &str {
        path.rsplit('/').next().unwrap_or("")
    }

    fn resolve_existing(&self, path: &str) -> Result<&Entry, Error> {
        if path == "/" {
            return Ok(self.entries.get("/").expect("root exists"));
        }

        let mut prefix = String::new();
        for (index, component) in path.split('/').filter(|part| !part.is_empty()).enumerate() {
            prefix.push('/');
            prefix.push_str(component);

            let entry = self
                .entries
                .get(&prefix)
                .ok_or_else(|| Error::Missing(prefix.clone()))?;

            if index + 1 < path.split('/').filter(|part| !part.is_empty()).count()
                && !matches!(entry, Entry::Dir)
            {
                return Err(Error::NotDir(prefix));
            }
        }

        self.entries
            .get(path)
            .ok_or_else(|| Error::Missing(path.to_string()))
    }

    pub fn mkdir(&mut self, path: &str) -> Result<(), Error> {
        let canonical = self.normalize(path)?;
        let parent = Self::parent_path(&canonical);

        match self.resolve_existing(parent)? {
            Entry::Dir => {}
            Entry::File(_) => return Err(Error::NotDir(parent.to_string())),
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

        let parent = Self::parent_path(&canonical);
        match self.resolve_existing(parent)? {
            Entry::Dir => {}
            Entry::File(_) => return Err(Error::NotDir(parent.to_string())),
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
        match self.resolve_existing(&canonical)? {
            Entry::Dir => Err(Error::IsDir(canonical)),
            Entry::File(contents) => Ok(contents.clone()),
        }
    }

    pub fn ls(&self, path: &str) -> Result<Vec<(String, Kind)>, Error> {
        let canonical = self.normalize(path)?;
        match self.resolve_existing(&canonical)? {
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
        match self.resolve_existing(&canonical)? {
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