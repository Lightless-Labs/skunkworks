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

impl Entry {
    fn kind(&self) -> Kind {
        match self {
            Entry::Dir => Kind::Dir,
            Entry::File(_) => Kind::File,
        }
    }
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
                if component.is_empty() || component == "." || component == ".." {
                    let offset = if component.is_empty() { i.saturating_sub(1) } else { start };
                    return Err(Error::InvalidPath(offset));
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

    fn resolve_existing(&self, components: &[&str]) -> Result<&Entry, Error> {
        let mut prefix = Vec::new();
        let mut current = self.entries.get("/").expect("root entry exists");

        for (i, component) in components.iter().enumerate() {
            if !matches!(current, Entry::Dir) {
                return Err(Error::NotDir(Self::canonical(&prefix)));
            }
            prefix.push(*component);
            let path = Self::canonical(&prefix);
            current = self
                .entries
                .get(&path)
                .ok_or_else(|| Error::Missing(path.clone()))?;
            if i + 1 < components.len() && !matches!(current, Entry::Dir) {
                return Err(Error::NotDir(path));
            }
        }

        Ok(current)
    }

    fn resolve_parent(&self, components: &[&str]) -> Result<String, Error> {
        let parent_components = &components[..components.len() - 1];
        let parent = Self::canonical(parent_components);
        let entry = self.resolve_existing(parent_components)?;
        if !matches!(entry, Entry::Dir) {
            return Err(Error::NotDir(parent));
        }
        Ok(parent)
    }

    pub fn mkdir(&mut self, path: &str) -> Result<(), Error> {
        let components = Self::validate(path)?;
        if components.is_empty() {
            return Err(Error::Exists("/".to_string()));
        }

        let parent = self.resolve_parent(&components)?;
        let target = Self::canonical(&components);
        if self.entries.contains_key(&target) {
            return Err(Error::Exists(target));
        }

        let _ = parent;
        self.entries.insert(target, Entry::Dir);
        Ok(())
    }

    pub fn write(&mut self, path: &str, data: &[u8]) -> Result<(), Error> {
        let components = Self::validate(path)?;
        if components.is_empty() {
            return Err(Error::IsDir("/".to_string()));
        }

        self.resolve_parent(&components)?;
        let target = Self::canonical(&components);
        match self.entries.get(&target) {
            Some(Entry::Dir) => Err(Error::IsDir(target)),
            Some(Entry::File(_)) | None => {
                self.entries
                    .insert(target, Entry::File(data.to_vec()));
                Ok(())
            }
        }
    }

    pub fn read(&self, path: &str) -> Result<Vec<u8>, Error> {
        let components = Self::validate(path)?;
        match self.resolve_existing(&components)? {
            Entry::Dir => Err(Error::IsDir(Self::canonical(&components))),
            Entry::File(data) => Ok(data.clone()),
        }
    }

    pub fn ls(&self, path: &str) -> Result<Vec<(String, Kind)>, Error> {
        let components = Self::validate(path)?;
        let resolved = Self::canonical(&components);
        match self.resolve_existing(&components)? {
            Entry::File(_) => return Err(Error::NotDir(resolved)),
            Entry::Dir => {}
        }

        let prefix = if resolved == "/" {
            "/".to_string()
        } else {
            format!("{}/", resolved)
        };

        let mut children = Vec::new();
        for (entry_path, entry) in &self.entries {
            if let Some(remainder) = entry_path.strip_prefix(&prefix) {
                if !remainder.is_empty() && !remainder.contains('/') {
                    children.push((remainder.to_string(), entry.kind()));
                }
            }
        }
        children.sort_by(|a, b| a.0.cmp(&b.0));
        Ok(children)
    }
}

fn main() {}