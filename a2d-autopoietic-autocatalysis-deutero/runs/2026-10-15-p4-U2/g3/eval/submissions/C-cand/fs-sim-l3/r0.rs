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
    Root,
    Busy(String),
    Cycle(String),
    NotEmpty(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
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

        let mut components: Vec<&str> = if path.starts_with('/') {
            Vec::new()
        } else {
            Self::components(&self.cwd)
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

        Ok(Self::canonical(&components))
    }

    fn components(path: &str) -> Vec<&str> {
        path.split('/').filter(|part| !part.is_empty()).collect()
    }

    fn canonical(components: &[&str]) -> String {
        if components.is_empty() {
            "/".to_owned()
        } else {
            format!("/{}", components.join("/"))
        }
    }

    fn parent_and_name(path: &str) -> Option<(String, String)> {
        if path == "/" {
            return None;
        }
        let split = path.rfind('/').expect("canonical paths contain a slash");
        let parent = if split == 0 {
            "/".to_owned()
        } else {
            path[..split].to_owned()
        };
        Some((parent, path[split + 1..].to_owned()))
    }

    fn resolve_directory(&self, path: &str) -> Result<(), Error> {
        let mut current = String::new();
        for component in Self::components(path) {
            current.push('/');
            current.push_str(component);
            match self.entries.get(&current) {
                None => return Err(Error::Missing(current)),
                Some(Entry::Dir) => {}
                Some(Entry::File(_)) => return Err(Error::NotDir(current)),
            }
        }
        Ok(())
    }

    fn resolve_parent(&self, path: &str) -> Result<(String, String), Error> {
        let (parent, name) =
            Self::parent_and_name(path).expect("root has no parent entry");
        self.resolve_directory(&parent)?;
        Ok((parent, name))
    }

    fn is_at_or_below(path: &str, ancestor: &str) -> bool {
        if ancestor == "/" {
            return path.starts_with('/');
        }
        path == ancestor
            || path
                .strip_prefix(ancestor)
                .is_some_and(|suffix| suffix.starts_with('/'))
    }

    fn has_children(&self, path: &str) -> bool {
        self.entries
            .keys()
            .any(|candidate| candidate != path && Self::is_at_or_below(candidate, path))
    }

    fn remove_subtree(&mut self, path: &str) {
        self.entries
            .retain(|candidate, _| !Self::is_at_or_below(candidate, path));
    }

    fn children(&self, path: &str) -> Vec<(String, Kind)> {
        let mut result = Vec::new();
        for (candidate, entry) in &self.entries {
            if candidate == path || !Self::is_at_or_below(candidate, path) {
                continue;
            }
            let Some((parent, name)) = Self::parent_and_name(candidate) else {
                continue;
            };
            if parent == path {
                let kind = match entry {
                    Entry::Dir => Kind::Dir,
                    Entry::File(_) => Kind::File,
                };
                result.push((name, kind));
            }
        }
        result.sort_by(|a, b| a.0.cmp(&b.0));
        result
    }

    pub fn mkdir(&mut self, path: &str) -> Result<(), Error> {
        let path = self.normalize(path)?;
        if path == "/" {
            return Err(Error::Exists(path));
        }
        let _ = self.resolve_parent(&path)?;
        if self.entries.contains_key(&path) {
            return Err(Error::Exists(path));
        }
        self.entries.insert(path, Entry::Dir);
        Ok(())
    }

    pub fn write(&mut self, path: &str, data: &[u8]) -> Result<(), Error> {
        let path = self.normalize(path)?;
        if path == "/" {
            return Err(Error::IsDir(path));
        }
        let _ = self.resolve_parent(&path)?;
        match self.entries.get(&path) {
            Some(Entry::Dir) => return Err(Error::IsDir(path)),
            Some(Entry::File(_)) | None => {}
        }
        self.entries
            .insert(path, Entry::File(data.to_vec()));
        Ok(())
    }

    pub fn read(&self, path: &str) -> Result<Vec<u8>, Error> {
        let path = self.normalize(path)?;
        if path == "/" {
            return Err(Error::IsDir(path));
        }
        let _ = self.resolve_parent(&path)?;
        match self.entries.get(&path) {
            None => Err(Error::Missing(path)),
            Some(Entry::Dir) => Err(Error::IsDir(path)),
            Some(Entry::File(data)) => Ok(data.clone()),
        }
    }

    pub fn ls(&self, path: &str) -> Result<Vec<(String, Kind)>, Error> {
        let path = self.normalize(path)?;
        self.resolve_directory(&path)?;
        match self.entries.get(&path) {
            Some(Entry::Dir) => Ok(self.children(&path)),
            Some(Entry::File(_)) => Err(Error::NotDir(path)),
            None => Err(Error::Missing(path)),
        }
    }

    pub fn cwd(&self) -> String {
        self.cwd.clone()
    }

    pub fn chdir(&mut self, path: &str) -> Result<(), Error> {
        let path = self.normalize(path)?;
        self.resolve_directory(&path)?;
        match self.entries.get(&path) {
            Some(Entry::Dir) => {
                self.cwd = path;
                Ok(())
            }
            Some(Entry::File(_)) => Err(Error::NotDir(path)),
            None => Err(Error::Missing(path)),
        }
    }

    pub fn rename(
        &mut self,
        from: &str,
        to: &str,
        overwrite: bool,
    ) -> Result<(), Error> {
        let from = self.normalize(from)?;
        let to = self.normalize(to)?;
        if from == "/" || to == "/" {
            return Err(Error::Root);
        }

        let _ = self.resolve_parent(&from)?;
        let source = self
            .entries
            .get(&from)
            .cloned()
            .ok_or_else(|| Error::Missing(from.clone()))?;

        if matches!(source, Entry::Dir) && Self::is_at_or_below(&self.cwd, &from) {
            return Err(Error::Busy(from));
        }

        let _ = self.resolve_parent(&to)?;
        let destination = self.entries.get(&to).cloned();

        if matches!(destination, Some(Entry::Dir))
            && Self::is_at_or_below(&self.cwd, &to)
        {
            return Err(Error::Busy(to));
        }

        if from == to {
            return Ok(());
        }

        if matches!(source, Entry::Dir) && Self::is_at_or_below(&to, &from) {
            return Err(Error::Cycle(to));
        }

        if destination.is_some() && !overwrite {
            return Err(Error::Exists(to));
        }

        if overwrite {
            match (&source, &destination) {
                (Entry::Dir, Some(Entry::File(_))) => {
                    return Err(Error::NotDir(to));
                }
                (Entry::File(_), Some(Entry::Dir)) => {
                    return Err(Error::IsDir(to));
                }
                (Entry::Dir, Some(Entry::Dir)) if self.has_children(&to) => {
                    return Err(Error::NotEmpty(to));
                }
                _ => {}
            }
        }

        let moved: Vec<(String, Entry)> = self
            .entries
            .iter()
            .filter(|(candidate, _)| Self::is_at_or_below(candidate, &from))
            .map(|(candidate, entry)| (candidate.clone(), entry.clone()))
            .collect();

        if destination.is_some() {
            self.remove_subtree(&to);
        }
        self.remove_subtree(&from);

        for (old_path, entry) in moved {
            let suffix = &old_path[from.len()..];
            let new_path = format!("{to}{suffix}");
            self.entries.insert(new_path, entry);
        }

        Ok(())
    }

    pub fn remove(&mut self, path: &str, recursive: bool) -> Result<(), Error> {
        let path = self.normalize(path)?;
        if path == "/" {
            return Err(Error::Root);
        }

        let _ = self.resolve_parent(&path)?;
        let entry = self
            .entries
            .get(&path)
            .cloned()
            .ok_or_else(|| Error::Missing(path.clone()))?;

        if matches!(entry, Entry::Dir) && Self::is_at_or_below(&self.cwd, &path) {
            return Err(Error::Busy(path));
        }

        if matches!(entry, Entry::Dir) && self.has_children(&path) && !recursive {
            return Err(Error::NotEmpty(path));
        }

        if matches!(entry, Entry::Dir) && recursive {
            self.remove_subtree(&path);
        } else {
            self.entries.remove(&path);
        }
        Ok(())
    }
}

fn main() {}