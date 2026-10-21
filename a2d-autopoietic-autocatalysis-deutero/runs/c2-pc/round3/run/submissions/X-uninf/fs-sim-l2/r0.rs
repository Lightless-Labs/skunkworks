use std::collections::HashMap;

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

#[derive(Debug, Clone, PartialEq, Eq)]
enum Entry {
    Dir,
    File(Vec<u8>),
}

pub struct Fs {
    entries: HashMap<String, Entry>,
    cwd: String,
}

impl Fs {
    pub fn new() -> Self {
        let mut entries = HashMap::new();
        entries.insert("/".to_owned(), Entry::Dir);
        Self {
            entries,
            cwd: "/".to_owned(),
        }
    }

    pub fn mkdir(&mut self, path: &str) -> Result<(), Error> {
        let canonical = self.resolve(path)?;
        if canonical == "/" {
            return Err(Error::Exists(canonical));
        }

        let (parent, _) = split_parent(&canonical);
        self.require_directory(&parent)?;

        if self.entries.contains_key(&canonical) {
            return Err(Error::Exists(canonical));
        }
        self.entries.insert(canonical, Entry::Dir);
        Ok(())
    }

    pub fn write(&mut self, path: &str, data: &[u8]) -> Result<(), Error> {
        let canonical = self.resolve(path)?;
        if canonical == "/" {
            return Err(Error::IsDir(canonical));
        }

        let (parent, _) = split_parent(&canonical);
        self.require_directory(&parent)?;

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
        let canonical = self.resolve(path)?;
        match self.lookup(&canonical)? {
            Entry::Dir => Err(Error::IsDir(canonical)),
            Entry::File(data) => Ok(data.clone()),
        }
    }

    pub fn ls(&self, path: &str) -> Result<Vec<(String, Kind)>, Error> {
        let canonical = self.resolve(path)?;
        match self.lookup(&canonical)? {
            Entry::File(_) => return Err(Error::NotDir(canonical)),
            Entry::Dir => {}
        }

        let prefix = if canonical == "/" {
            "/".to_owned()
        } else {
            format!("{canonical}/")
        };

        let mut children = Vec::new();
        for (entry_path, entry) in &self.entries {
            if let Some(rest) = entry_path.strip_prefix(&prefix) {
                if !rest.is_empty() && !rest.contains('/') {
                    let kind = match entry {
                        Entry::Dir => Kind::Dir,
                        Entry::File(_) => Kind::File,
                    };
                    children.push((rest.to_owned(), kind));
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
        let canonical = self.resolve(path)?;
        match self.lookup(&canonical)? {
            Entry::Dir => {
                self.cwd = canonical;
                Ok(())
            }
            Entry::File(_) => Err(Error::NotDir(canonical)),
        }
    }

    fn resolve(&self, path: &str) -> Result<String, Error> {
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
            Ok(format!("/{}", components.join("/")))
        }
    }

    fn require_directory(&self, path: &str) -> Result<(), Error> {
        match self.lookup(path)? {
            Entry::Dir => Ok(()),
            Entry::File(_) => Err(Error::NotDir(path.to_owned())),
        }
    }

    fn lookup(&self, canonical: &str) -> Result<&Entry, Error> {
        if canonical == "/" {
            return self
                .entries
                .get("/")
                .ok_or_else(|| Error::Missing("/".to_owned()));
        }

        let mut prefix = String::new();
        let components: Vec<&str> = canonical
            .split('/')
            .filter(|component| !component.is_empty())
            .collect();

        for (index, component) in components.iter().enumerate() {
            prefix.push('/');
            prefix.push_str(component);

            let entry = self
                .entries
                .get(&prefix)
                .ok_or_else(|| Error::Missing(prefix.clone()))?;

            if index + 1 < components.len() && matches!(entry, Entry::File(_)) {
                return Err(Error::NotDir(prefix));
            }
        }

        self.entries
            .get(canonical)
            .ok_or_else(|| Error::Missing(canonical.to_owned()))
    }
}

fn split_parent(canonical: &str) -> (String, &str) {
    match canonical.rfind('/') {
        Some(0) => ("/".to_owned(), &canonical[1..]),
        Some(index) => (canonical[..index].to_owned(), &canonical[index + 1..]),
        None => ("/".to_owned(), canonical),
    }
}