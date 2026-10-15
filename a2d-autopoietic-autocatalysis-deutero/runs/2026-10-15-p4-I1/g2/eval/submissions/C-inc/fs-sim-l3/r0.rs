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

#[derive(Clone)]
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
    entries: std::collections::BTreeMap<String, Entry>,
    cwd: String,
}

impl Fs {
    pub fn new() -> Self {
        let mut entries = std::collections::BTreeMap::new();
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

        let mut parts: Vec<&str> = Vec::new();
        if !path.starts_with('/') {
            parts.extend(self.cwd.split('/').filter(|s| !s.is_empty()));
        }

        for part in path.split('/') {
            match part {
                "" | "." => {}
                ".." => {
                    parts.pop();
                }
                other => parts.push(other),
            }
        }

        if parts.is_empty() {
            Ok("/".to_string())
        } else {
            Ok(format!("/{}", parts.join("/")))
        }
    }

    fn parent(path: &str) -> (&str, &str) {
        if path == "/" {
            return ("/", "");
        }
        match path.rfind('/') {
            Some(0) => ("/", &path[1..]),
            Some(i) => (&path[..i], &path[i + 1..]),
            None => ("/", path),
        }
    }

    fn is_within(path: &str, dir: &str) -> bool {
        if dir == "/" {
            return path.starts_with('/');
        }
        path == dir
            || path
                .strip_prefix(dir)
                .is_some_and(|suffix| suffix.starts_with('/'))
    }

    fn check_parent(&self, path: &str) -> Result<(), Error> {
        let (parent, _) = Self::parent(path);
        match self.entries.get(parent) {
            None => Err(Error::Missing(parent.to_string())),
            Some(Entry::Dir) => Ok(()),
            Some(Entry::File(_)) => Err(Error::NotDir(parent.to_string())),
        }
    }

    fn check_traversal(&self, path: &str) -> Result<(), Error> {
        if path == "/" {
            return Ok(());
        }
        let mut prefix = String::new();
        for component in path.split('/').filter(|s| !s.is_empty()) {
            prefix.push('/');
            prefix.push_str(component);
            match self.entries.get(&prefix) {
                None => return Err(Error::Missing(prefix)),
                Some(Entry::File(_)) if prefix != path => {
                    return Err(Error::NotDir(prefix));
                }
                _ => {}
            }
        }
        Ok(())
    }

    fn resolve_parent(&self, path: &str) -> Result<(), Error> {
        let (parent, _) = Self::parent(path);
        self.check_traversal(parent)?;
        match self.entries.get(parent) {
            Some(Entry::Dir) => Ok(()),
            Some(Entry::File(_)) => Err(Error::NotDir(parent.to_string())),
            None => Err(Error::Missing(parent.to_string())),
        }
    }

    pub fn mkdir(&mut self, path: &str) -> Result<(), Error> {
        let path = self.normalize(path)?;
        self.resolve_parent(&path)?;
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
        self.resolve_parent(&path)?;
        match self.entries.get(&path) {
            Some(Entry::Dir) => Err(Error::IsDir(path)),
            Some(Entry::File(_)) | None => {
                self.entries
                    .insert(path, Entry::File(data.to_vec()));
                Ok(())
            }
        }
    }

    pub fn read(&self, path: &str) -> Result<Vec<u8>, Error> {
        let path = self.normalize(path)?;
        self.check_traversal(&path)?;
        match self.entries.get(&path) {
            None => Err(Error::Missing(path)),
            Some(Entry::Dir) => Err(Error::IsDir(path)),
            Some(Entry::File(bytes)) => Ok(bytes.clone()),
        }
    }

    pub fn ls(&self, path: &str) -> Result<Vec<(String, Kind)>, Error> {
        let path = self.normalize(path)?;
        self.check_traversal(&path)?;
        match self.entries.get(&path) {
            None => return Err(Error::Missing(path)),
            Some(Entry::File(_)) => return Err(Error::NotDir(path)),
            Some(Entry::Dir) => {}
        }

        let prefix = if path == "/" {
            "/".to_string()
        } else {
            format!("{}/", path)
        };

        let mut children = Vec::new();
        for (candidate, entry) in &self.entries {
            if candidate == &path || !candidate.starts_with(&prefix) {
                continue;
            }
            let remainder = &candidate[prefix.len()..];
            if !remainder.is_empty() && !remainder.contains('/') {
                children.push((remainder.to_string(), entry.kind()));
            }
        }
        children.sort_by(|a, b| a.0.cmp(&b.0));
        Ok(children)
    }

    pub fn cwd(&self) -> String {
        self.cwd.clone()
    }

    pub fn chdir(&mut self, path: &str) -> Result<(), Error> {
        let path = self.normalize(path)?;
        self.check_traversal(&path)?;
        match self.entries.get(&path) {
            Some(Entry::Dir) => {
                self.cwd = path;
                Ok(())
            }
            _ => Err(Error::NotDir(path)),
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

        self.resolve_parent(&from)?;
        let source = self
            .entries
            .get(&from)
            .cloned()
            .ok_or_else(|| Error::Missing(from.clone()))?;

        if matches!(source, Entry::Dir) && Self::is_within(&self.cwd, &from) {
            return Err(Error::Busy(from));
        }

        self.resolve_parent(&to)?;

        if matches!(self.entries.get(&to), Some(Entry::Dir))
            && Self::is_within(&self.cwd, &to)
        {
            return Err(Error::Busy(to));
        }

        if from == to {
            return Ok(());
        }

        if matches!(source, Entry::Dir) && Self::is_within(&to, &from) {
            return Err(Error::Cycle(to));
        }

        let destination = self.entries.get(&to).cloned();
        if destination.is_some() && !overwrite {
            return Err(Error::Exists(to));
        }

        if overwrite {
            if let Some(dest) = &destination {
                match (&source, dest) {
                    (Entry::Dir, Entry::File(_)) => return Err(Error::NotDir(to)),
                    (Entry::File(_), Entry::Dir) => return Err(Error::IsDir(to)),
                    (Entry::Dir, Entry::Dir) => {
                        let prefix = format!("{}/", to);
                        if self.entries.keys().any(|p| p.starts_with(&prefix)) {
                            return Err(Error::NotEmpty(to));
                        }
                    }
                    _ => {}
                }
            }
        }

        let moved: Vec<(String, Entry)> = self
            .entries
            .iter()
            .filter(|(p, _)| *p == &from || Self::is_within(p, &from))
            .map(|(p, e)| (p.clone(), e.clone()))
            .collect();

        if let Some(dest) = destination {
            if matches!(dest, Entry::Dir) {
                self.entries.remove(&to);
            } else {
                self.entries.remove(&to);
            }
        }

        for (old_path, _) in &moved {
            self.entries.remove(old_path);
        }

        for (old_path, entry) in moved {
            let suffix = &old_path[from.len()..];
            self.entries.insert(format!("{}{}", to, suffix), entry);
        }
        Ok(())
    }

    pub fn remove(&mut self, path: &str, recursive: bool) -> Result<(), Error> {
        let path = self.normalize(path)?;
        if path == "/" {
            return Err(Error::Root);
        }

        self.resolve_parent(&path)?;
        let entry = self
            .entries
            .get(&path)
            .cloned()
            .ok_or_else(|| Error::Missing(path.clone()))?;

        if matches!(entry, Entry::Dir) && Self::is_within(&self.cwd, &path) {
            return Err(Error::Busy(path));
        }

        let prefix = format!("{}/", path);
        let has_children = self.entries.keys().any(|p| p.starts_with(&prefix));
        if matches!(entry, Entry::Dir) && has_children && !recursive {
            return Err(Error::NotEmpty(path));
        }

        if recursive && matches!(entry, Entry::Dir) {
            let victims: Vec<String> = self
                .entries
                .keys()
                .filter(|p| *p == &path || Self::is_within(p, &path))
                .cloned()
                .collect();
            for victim in victims {
                self.entries.remove(&victim);
            }
        } else {
            self.entries.remove(&path);
        }
        Ok(())
    }
}

fn main() {}