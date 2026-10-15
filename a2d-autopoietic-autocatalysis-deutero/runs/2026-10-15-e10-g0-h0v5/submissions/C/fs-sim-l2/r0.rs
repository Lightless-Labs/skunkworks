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

        let mut components: Vec<&str> = if path.starts_with('/') {
            Vec::new()
        } else {
            self.cwd
                .split('/')
                .filter(|component| !component.is_empty())
                .collect()
        };

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
            Some(0) => "/",
            Some(i) => &path[..i],
            _ => "/",
        }
    }

    fn check_directory(&self, path: &str) -> Result<(), Error> {
        if path == "/" {
            return Ok(());
        }

        let mut current = String::from("/");
        let components: Vec<&str> = path.split('/').filter(|c| !c.is_empty()).collect();

        for component in components {
            if current != "/" {
                current.push('/');
            }
            current.push_str(component);

            match self.entries.get(&current) {
                None => return Err(Error::Missing(current)),
                Some(Entry::File(_)) => return Err(Error::NotDir(current)),
                Some(Entry::Dir) => {}
            }
        }

        Ok(())
    }

    fn entry_kind(entry: &Entry) -> Kind {
        match entry {
            Entry::Dir => Kind::Dir,
            Entry::File(_) => Kind::File,
        }
    }

    pub fn mkdir(&mut self, path: &str) -> Result<(), Error> {
        let canonical = self.normalize(path)?;
        if canonical == "/" {
            return Err(Error::Exists(canonical));
        }

        let parent = Self::parent_path(&canonical);
        self.check_directory(parent)?;

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
        self.check_directory(parent)?;

        match self.entries.get(&canonical) {
            Some(Entry::Dir) => return Err(Error::IsDir(canonical)),
            Some(Entry::File(_)) | None => {}
        }

        self.entries.insert(canonical, Entry::File(data.to_vec()));
        Ok(())
    }

    pub fn read(&self, path: &str) -> Result<Vec<u8>, Error> {
        let canonical = self.normalize(path)?;
        let parent = Self::parent_path(&canonical);
        self.check_directory(parent)?;

        match self.entries.get(&canonical) {
            Some(Entry::File(data)) => Ok(data.clone()),
            Some(Entry::Dir) => Err(Error::IsDir(canonical)),
            None => Err(Error::Missing(canonical)),
        }
    }

    pub fn ls(&self, path: &str) -> Result<Vec<(String, Kind)>, Error> {
        let canonical = self.normalize(path)?;
        self.check_directory(&canonical)?;

        match self.entries.get(&canonical) {
            Some(Entry::File(_)) => return Err(Error::NotDir(canonical)),
            Some(Entry::Dir) => {}
            None => return Err(Error::Missing(canonical)),
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
                    children.push((rest.to_string(), Self::entry_kind(entry)));
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
        self.check_directory(&canonical)?;

        match self.entries.get(&canonical) {
            Some(Entry::Dir) => {
                self.cwd = canonical;
                Ok(())
            }
            Some(Entry::File(_)) => Err(Error::NotDir(canonical)),
            None => Err(Error::Missing(canonical)),
        }
    }
}

fn main() {}