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
    entries: std::collections::HashMap<String, Entry>,
}

impl Fs {
    pub fn new() -> Self {
        let mut entries = std::collections::HashMap::new();
        entries.insert("/".to_string(), Entry::Dir);
        Self { entries }
    }

    fn validate(path: &str) -> Result<Vec<String>, Error> {
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
        for (i, b) in path.bytes().enumerate().skip(1) {
            if b == b'/' {
                let component = &path[start..i];
                if component.is_empty() {
                    return Err(Error::InvalidPath(i));
                }
                if component == "." || component == ".." {
                    return Err(Error::InvalidPath(start));
                }
                components.push(component.to_string());
                start = i + 1;
            }
        }

        let component = &path[start..];
        if component.is_empty() {
            return Err(Error::InvalidPath(path.len() - 1));
        }
        if component == "." || component == ".." {
            return Err(Error::InvalidPath(start));
        }
        components.push(component.to_string());
        Ok(components)
    }

    fn canonical(components: &[String]) -> String {
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

    fn check_directory(&self, components: &[String]) -> Result<(), Error> {
        let mut prefix = Vec::new();
        for component in components {
            let parent_path = Self::canonical(&prefix);
            match self.entries.get(&parent_path) {
                Some(Entry::Dir) => {}
                Some(Entry::File(_)) => return Err(Error::NotDir(parent_path)),
                None => return Err(Error::Missing(parent_path)),
            }
            prefix.push(component.clone());
        }
        let path = Self::canonical(components);
        match self.entries.get(&path) {
            Some(Entry::Dir) => Ok(()),
            Some(Entry::File(_)) => Err(Error::NotDir(path)),
            None => Err(Error::Missing(path)),
        }
    }

    fn check_parent(&self, components: &[String]) -> Result<String, Error> {
        let parent_components = &components[..components.len() - 1];
        self.check_directory(parent_components)?;
        Ok(Self::canonical(parent_components))
    }

    pub fn mkdir(&mut self, path: &str) -> Result<(), Error> {
        let components = Self::validate(path)?;
        if components.is_empty() {
            return Err(Error::Exists("/".to_string()));
        }

        let parent = self.check_parent(&components)?;
        let canonical = Self::canonical(&components);
        if self.entries.contains_key(&canonical) {
            return Err(Error::Exists(canonical));
        }

        let _ = parent;
        self.entries.insert(canonical, Entry::Dir);
        Ok(())
    }

    pub fn write(&mut self, path: &str, data: &[u8]) -> Result<(), Error> {
        let components = Self::validate(path)?;
        if components.is_empty() {
            return Err(Error::IsDir("/".to_string()));
        }

        self.check_parent(&components)?;
        let canonical = Self::canonical(&components);
        match self.entries.get(&canonical) {
            Some(Entry::Dir) => Err(Error::IsDir(canonical)),
            Some(Entry::File(_)) | None => {
                self.entries
                    .insert(canonical, Entry::File(data.to_vec()));
                Ok(())
            }
        }
    }

    pub fn read(&self, path: &str) -> Result<Vec<u8>, Error> {
        let components = Self::validate(path)?;
        if components.is_empty() {
            return Err(Error::IsDir("/".to_string()));
        }

        let canonical = Self::canonical(&components);
        let parent_components = &components[..components.len() - 1];
        self.check_directory(parent_components)?;
        match self.entries.get(&canonical) {
            Some(Entry::File(data)) => Ok(data.clone()),
            Some(Entry::Dir) => Err(Error::IsDir(canonical)),
            None => Err(Error::Missing(canonical)),
        }
    }

    pub fn ls(&self, path: &str) -> Result<Vec<(String, Kind)>, Error> {
        let components = Self::validate(path)?;
        let canonical = Self::canonical(&components);
        self.check_directory(&components)?;

        let mut children = Vec::new();
        for (entry_path, entry) in &self.entries {
            if entry_path == "/" {
                continue;
            }

            let parent = match entry_path.rsplit_once('/') {
                Some((parent, _)) if parent.is_empty() => "/",
                Some((parent, _)) => parent,
                None => continue,
            };

            if parent == canonical {
                let basename = entry_path
                    .rsplit_once('/')
                    .map(|(_, name)| name.to_string())
                    .unwrap_or_default();
                let kind = match entry {
                    Entry::Dir => Kind::Dir,
                    Entry::File(_) => Kind::File,
                };
                children.push((basename, kind));
            }
        }

        children.sort_by(|a, b| a.0.cmp(&b.0));
        Ok(children)
    }
}