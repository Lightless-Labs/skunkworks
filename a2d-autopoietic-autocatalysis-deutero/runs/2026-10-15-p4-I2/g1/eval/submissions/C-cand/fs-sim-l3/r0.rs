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
            components.extend(self.cwd.split('/').filter(|s| !s.is_empty()));
        }

        for part in path.split('/') {
            match part {
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
            let mut result = String::new();
            for component in components {
                result.push('/');
                result.push_str(component);
            }
            Ok(result)
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

    fn resolve_parent(&self, path: &str) -> Result<(), Error> {
        let (parent, _) = Self::parent(path);
        self.resolve_existing(parent)?;
        match self.entries.get(parent) {
            Some(Entry::Dir) => Ok(()),
            Some(Entry::File(_)) => Err(Error::NotDir(parent.to_string())),
            None => Err(Error::Missing(parent.to_string())),
        }
    }

    fn resolve_existing(&self, path: &str) -> Result<&Entry, Error> {
        if path == "/" {
            return self
                .entries
                .get("/")
                .ok_or_else(|| Error::Missing("/".to_string()));
        }

        let mut prefix = String::new();
        for component in path.split('/').filter(|s| !s.is_empty()) {
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
            .ok_or_else(|| Error::Missing(path.to_string()))
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

    fn children(&self, path: &str) -> Vec<String> {
        self.entries
            .keys()
            .filter(|candidate| {
                if candidate.as_str() == path || !Self::is_at_or_below(candidate, path) {
                    return false;
                }
                let rest = if path == "/" {
                    &candidate[1..]
                } else {
                    &candidate[path.len() + 1..]
                };
                !rest.is_empty() && !rest.contains('/')
            })
            .cloned()
            .collect()
    }

    fn subtree_entries(&self, path: &str) -> Vec<(String, Entry)> {
        self.entries
            .iter()
            .filter(|(candidate, _)| Self::is_at_or_below(candidate, path))
            .map(|(k, v)| (k.clone(), v.clone()))
            .collect()
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
        self.resolve_parent(&path)?;
        match self.entries.get(&path) {
            Some(Entry::Dir) => return Err(Error::IsDir(path)),
            Some(Entry::File(_)) => {}
            None => {}
        }
        self.entries
            .insert(path, Entry::File(data.to_vec()));
        Ok(())
    }

    pub fn read(&self, path: &str) -> Result<Vec<u8>, Error> {
        let path = self.normalize(path)?;
        match self.resolve_existing(&path)? {
            Entry::Dir => Err(Error::IsDir(path)),
            Entry::File(data) => Ok(data.clone()),
        }
    }

    pub fn ls(&self, path: &str) -> Result<Vec<(String, Kind)>, Error> {
        let path = self.normalize(path)?;
        match self.resolve_existing(&path)? {
            Entry::File(_) => return Err(Error::NotDir(path)),
            Entry::Dir => {}
        }

        let mut result = Vec::new();
        for child in self.children(&path) {
            let name = if path == "/" {
                child[1..].to_string()
            } else {
                child[path.len() + 1..].to_string()
            };
            let kind = self.entries.get(&child).expect("listed child exists").kind();
            result.push((name, kind));
        }
        result.sort_by(|a, b| a.0.cmp(&b.0));
        Ok(result)
    }

    pub fn cwd(&self) -> String {
        self.cwd.clone()
    }

    pub fn chdir(&mut self, path: &str) -> Result<(), Error> {
        let path = self.normalize(path)?;
        match self.resolve_existing(&path)? {
            Entry::Dir => {
                self.cwd = path;
                Ok(())
            }
            Entry::File(_) => Err(Error::NotDir(path)),
        }
    }

    pub fn rename(&mut self, from: &str, to: &str, overwrite: bool) -> Result<(), Error> {
        let from = self.normalize(from)?;
        let to = self.normalize(to)?;

        if from == "/" || to == "/" {
            return Err(Error::Root);
        }

        self.resolve_parent(&from)?;
        if !self.entries.contains_key(&from) {
            return Err(Error::Missing(from));
        }

        let source_is_dir = matches!(self.entries.get(&from), Some(Entry::Dir));
        if source_is_dir && Self::is_at_or_below(&self.cwd, &from) {
            return Err(Error::Busy(from));
        }

        self.resolve_parent(&to)?;

        let dest_entry = self.entries.get(&to);
        if matches!(dest_entry, Some(Entry::Dir))
            && Self::is_at_or_below(&self.cwd, &to)
        {
            return Err(Error::Busy(to));
        }

        if from == to {
            return Ok(());
        }

        if source_is_dir && Self::is_at_or_below(&to, &from) {
            return Err(Error::Cycle(to));
        }

        if dest_entry.is_some() && !overwrite {
            return Err(Error::Exists(to));
        }

        if overwrite {
            if let Some(destination) = dest_entry {
                let dest_is_dir = matches!(destination, Entry::Dir);
                match (source_is_dir, dest_is_dir) {
                    (true, false) => return Err(Error::NotDir(to)),
                    (false, true) => return Err(Error::IsDir(to)),
                    (true, true) => {
                        if !self.children(&to).is_empty() {
                            return Err(Error::NotEmpty(to));
                        }
                    }
                    (false, false) => {}
                }
            }
        }

        let source_entries = self.subtree_entries(&from);
        let destination_entries = if dest_entry.is_some() {
            self.subtree_entries(&to)
        } else {
            Vec::new()
        };

        for (path, _) in &destination_entries {
            self.entries.remove(path);
        }
        for (path, _) in &source_entries {
            self.entries.remove(path);
        }

        for (old_path, entry) in source_entries {
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

        self.resolve_parent(&path)?;
        if !self.entries.contains_key(&path) {
            return Err(Error::Missing(path));
        }

        if matches!(self.entries.get(&path), Some(Entry::Dir))
            && Self::is_at_or_below(&self.cwd, &path)
        {
            return Err(Error::Busy(path));
        }

        let children = self.children(&path);
        if matches!(self.entries.get(&path), Some(Entry::Dir))
            && !children.is_empty()
            && !recursive
        {
            return Err(Error::NotEmpty(path));
        }

        let targets = if recursive {
            self.subtree_entries(&path)
                .into_iter()
                .map(|(name, _)| name)
                .collect::<Vec<_>>()
        } else {
            vec![path]
        };

        for target in targets {
            self.entries.remove(&target);
        }
        Ok(())
    }
}

fn main() {}