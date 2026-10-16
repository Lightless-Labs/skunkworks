use std::collections::{BTreeMap, HashSet};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    Dir,
    File,
    Link,
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
    InvalidTarget(usize),
    Loop(String),
    Depth(String),
}

#[derive(Debug, Clone)]
enum Entry {
    Dir,
    File(Vec<u8>),
    Link(String),
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

    fn validate(path: &str) -> Result<(), Error> {
        if let Some(i) = path.as_bytes().iter().position(|b| *b == 0) {
            return Err(Error::InvalidPath(i));
        }
        if path.is_empty() {
            return Err(Error::InvalidPath(0));
        }
        Ok(())
    }

    fn components(path: &str) -> Vec<String> {
        path.split('/')
            .filter(|c| !c.is_empty() && *c != ".")
            .fold(Vec::new(), |mut acc, c| {
                if c == ".." {
                    acc.pop();
                } else {
                    acc.push(c.to_string());
                }
                acc
            })
    }

    fn canonical(parts: &[String]) -> String {
        if parts.is_empty() {
            "/".to_string()
        } else {
            format!("/{}", parts.join("/"))
        }
    }

    fn normalize(path: &str, cwd: &str) -> String {
        let mut parts = if path.starts_with('/') {
            Vec::new()
        } else {
            Self::components(cwd)
        };
        for c in path.split('/') {
            if c.is_empty() || c == "." {
                continue;
            } else if c == ".." {
                parts.pop();
            } else {
                parts.push(c.to_string());
            }
        }
        Self::canonical(&parts)
    }

    fn parent(path: &str) -> String {
        if path == "/" {
            return "/".to_string();
        }
        match path.rfind('/') {
            Some(0) => "/".to_string(),
            Some(i) => path[..i].to_string(),
            _ => "/".to_string(),
        }
    }

    fn basename(path: &str) -> String {
        if path == "/" {
            "/".to_string()
        } else {
            path.rsplit('/').next().unwrap_or("").to_string()
        }
    }

    fn is_within(path: &str, dir: &str) -> bool {
        if dir == "/" {
            path.starts_with('/')
        } else {
            path == dir || path.strip_prefix(dir).is_some_and(|s| s.starts_with('/'))
        }
    }

    fn kind(entry: &Entry) -> Kind {
        match entry {
            Entry::Dir => Kind::Dir,
            Entry::File(_) => Kind::File,
            Entry::Link(_) => Kind::Link,
        }
    }

    fn resolve(&self, path: &str, follow_final: bool, allow_missing_final: bool) -> Result<String, Error> {
        let mut pending = Self::components(path);
        let mut physical: Vec<String> = Vec::new();
        let mut visited = HashSet::new();
        let mut expansions = 0usize;

        while !pending.is_empty() {
            let name = pending.remove(0);
            let candidate_parts = {
                let mut v = physical.clone();
                v.push(name);
                v
            };
            let candidate = Self::canonical(&candidate_parts);
            let is_final = pending.is_empty();

            match self.entries.get(&candidate) {
                None if is_final && allow_missing_final => return Ok(candidate),
                None => return Err(Error::Missing(candidate)),
                Some(Entry::Link(target)) if !is_final || follow_final => {
                    if !visited.insert(candidate.clone()) {
                        return Err(Error::Loop(candidate));
                    }
                    if expansions >= 8 {
                        return Err(Error::Depth(candidate));
                    }
                    expansions += 1;

                    let mut target_parts = if target.starts_with('/') {
                        Vec::new()
                    } else {
                        physical.clone()
                    };
                    for c in target.split('/') {
                        if c.is_empty() || c == "." {
                            continue;
                        } else if c == ".." {
                            target_parts.pop();
                        } else {
                            target_parts.push(c.to_string());
                        }
                    }
                    let mut expanded = target_parts;
                    expanded.extend(pending.into_iter());
                    pending = expanded;
                    physical.clear();
                }
                Some(entry) => {
                    if !is_final && !matches!(entry, Entry::Dir) {
                        return Err(Error::NotDir(candidate));
                    }
                    physical.push(Self::basename(&candidate));
                }
            }
        }
        Ok(Self::canonical(&physical))
    }

    fn resolve_parent(&self, path: &str) -> Result<(String, String), Error> {
        let parent = Self::parent(path);
        let resolved_parent = self.resolve(&parent, true, false)?;
        match self.entries.get(&resolved_parent) {
            Some(Entry::Dir) => Ok((resolved_parent, Self::basename(path))),
            Some(_) => Err(Error::NotDir(resolved_parent)),
            None => Err(Error::Missing(resolved_parent)),
        }
    }

    fn child_paths(&self, path: &str) -> Vec<String> {
        self.entries
            .keys()
            .filter(|candidate| *candidate != path && Self::parent(candidate) == path)
            .cloned()
            .collect()
    }

    fn subtree_paths(&self, path: &str) -> Vec<String> {
        self.entries
            .keys()
            .filter(|candidate| Self::is_within(candidate, path))
            .cloned()
            .collect()
    }

    pub fn mkdir(&mut self, path: &str) -> Result<(), Error> {
        Self::validate(path)?;
        let normalized = Self::normalize(path, &self.cwd);
        if normalized == "/" {
            return Err(Error::Exists("/".to_string()));
        }
        let (parent, name) = self.resolve_parent(&normalized)?;
        let final_path = if parent == "/" {
            format!("/{}", name)
        } else {
            format!("{}/{}", parent, name)
        };
        if self.entries.contains_key(&final_path) {
            return Err(Error::Exists(final_path));
        }
        self.entries.insert(final_path, Entry::Dir);
        Ok(())
    }

    pub fn write(&mut self, path: &str, data: &[u8]) -> Result<(), Error> {
        Self::validate(path)?;
        let normalized = Self::normalize(path, &self.cwd);
        let resolved = self.resolve(&normalized, true, true)?;
        if resolved == "/" {
            return Err(Error::IsDir("/".to_string()));
        }
        match self.entries.get(&resolved) {
            Some(Entry::Dir) => return Err(Error::IsDir(resolved)),
            Some(Entry::File(_)) => {}
            Some(Entry::Link(_)) => unreachable!(),
            None => {
                let parent = Self::parent(&resolved);
                let parent_resolved = self.resolve(&parent, true, false)?;
                match self.entries.get(&parent_resolved) {
                    Some(Entry::Dir) => {}
                    Some(_) => return Err(Error::NotDir(parent_resolved)),
                    None => return Err(Error::Missing(parent_resolved)),
                }
            }
        }
        self.entries.insert(resolved, Entry::File(data.to_vec()));
        Ok(())
    }

    pub fn read(&self, path: &str) -> Result<Vec<u8>, Error> {
        Self::validate(path)?;
        let normalized = Self::normalize(path, &self.cwd);
        let resolved = self.resolve(&normalized, true, false)?;
        match self.entries.get(&resolved) {
            Some(Entry::File(data)) => Ok(data.clone()),
            Some(Entry::Dir) => Err(Error::IsDir(resolved)),
            Some(Entry::Link(_)) => unreachable!(),
            None => Err(Error::Missing(resolved)),
        }
    }

    pub fn ls(&self, path: &str) -> Result<Vec<(String, Kind)>, Error> {
        Self::validate(path)?;
        let normalized = Self::normalize(path, &self.cwd);
        let resolved = self.resolve(&normalized, true, false)?;
        if !matches!(self.entries.get(&resolved), Some(Entry::Dir)) {
            return Err(Error::NotDir(resolved));
        }
        let mut result: Vec<_> = self
            .entries
            .iter()
            .filter(|(candidate, _)| *candidate != &resolved && Self::parent(candidate) == resolved)
            .map(|(candidate, entry)| (Self::basename(candidate), Self::kind(entry)))
            .collect();
        result.sort_by(|a, b| a.0.cmp(&b.0));
        Ok(result)
    }

    pub fn cwd(&self) -> String {
        self.cwd.clone()
    }

    pub fn chdir(&mut self, path: &str) -> Result<(), Error> {
        Self::validate(path)?;
        let normalized = Self::normalize(path, &self.cwd);
        let resolved = self.resolve(&normalized, true, false)?;
        if !matches!(self.entries.get(&resolved), Some(Entry::Dir)) {
            return Err(Error::NotDir(resolved));
        }
        self.cwd = resolved;
        Ok(())
    }

    pub fn rename(&mut self, from: &str, to: &str, overwrite: bool) -> Result<(), Error> {
        Self::validate(from)?;
        let source = Self::normalize(from, &self.cwd);
        Self::validate(to)?;
        let destination = Self::normalize(to, &self.cwd);
        if source == "/" || destination == "/" {
            return Err(Error::Root);
        }

        let (source_parent, source_name) = self.resolve_parent(&source)?;
        let source_path = if source_parent == "/" {
            format!("/{}", source_name)
        } else {
            format!("{}/{}", source_parent, source_name)
        };
        let source_entry = self.entries.get(&source_path).cloned().ok_or_else(|| Error::Missing(source_path.clone()))?;
        if matches!(source_entry, Entry::Dir) && Self::is_within(&self.cwd, &source_path) {
            return Err(Error::Busy(source_path));
        }

        let (destination_parent, destination_name) = self.resolve_parent(&destination)?;
        let destination_path = if destination_parent == "/" {
            format!("/{}", destination_name)
        } else {
            format!("{}/{}", destination_parent, destination_name)
        };
        if matches!(self.entries.get(&destination_path), Some(Entry::Dir))
            && Self::is_within(&self.cwd, &destination_path)
        {
            return Err(Error::Busy(destination_path));
        }

        if source_path == destination_path {
            return Ok(());
        }
        if matches!(source_entry, Entry::Dir) && Self::is_within(&destination_path, &source_path) {
            return Err(Error::Cycle(destination_path));
        }

        let destination_entry = self.entries.get(&destination_path).cloned();
        if destination_entry.is_some() && !overwrite {
            return Err(Error::Exists(destination_path));
        }
        if let Some(dest) = &destination_entry {
            match (&source_entry, dest) {
                (Entry::Dir, Entry::Dir) => {
                    if !self.child_paths(&destination_path).is_empty() {
                        return Err(Error::NotEmpty(destination_path));
                    }
                }
                (Entry::Dir, _) => return Err(Error::NotDir(destination_path)),
                (_, Entry::Dir) => return Err(Error::IsDir(destination_path)),
                _ => {}
            }
        }

        let moving = self.subtree_paths(&source_path);
        if destination_entry.is_some() {
            let replaced = self.subtree_paths(&destination_path);
            for p in replaced {
                self.entries.remove(&p);
            }
        }
        let mut moved_entries = Vec::new();
        for old in &moving {
            if let Some(entry) = self.entries.remove(old) {
                let suffix = &old[source_path.len()..];
                moved_entries.push((format!("{}{}", destination_path, suffix), entry));
            }
        }
        for (path, entry) in moved_entries {
            self.entries.insert(path, entry);
        }
        Ok(())
    }

    pub fn remove(&mut self, path: &str, recursive: bool) -> Result<(), Error> {
        Self::validate(path)?;
        let normalized = Self::normalize(path, &self.cwd);
        if normalized == "/" {
            return Err(Error::Root);
        }
        let (parent, name) = self.resolve_parent(&normalized)?;
        let final_path = if parent == "/" {
            format!("/{}", name)
        } else {
            format!("{}/{}", parent, name)
        };
        let entry = self.entries.get(&final_path).ok_or_else(|| Error::Missing(final_path.clone()))?;
        if matches!(entry, Entry::Dir) && Self::is_within(&self.cwd, &final_path) {
            return Err(Error::Busy(final_path));
        }
        if matches!(entry, Entry::Dir) && !self.child_paths(&final_path).is_empty() && !recursive {
            return Err(Error::NotEmpty(final_path));
        }
        for p in self.subtree_paths(&final_path) {
            self.entries.remove(&p);
        }
        Ok(())
    }

    pub fn symlink(&mut self, target: &str, link: &str) -> Result<(), Error> {
        Self::validate(link)?;
        let normalized = Self::normalize(link, &self.cwd);
        if let Some(i) = target.as_bytes().iter().position(|b| *b == 0) {
            return Err(Error::InvalidTarget(i));
        }
        if target.is_empty() {
            return Err(Error::InvalidTarget(0));
        }
        if normalized == "/" {
            return Err(Error::Exists("/".to_string()));
        }
        let (parent, name) = self.resolve_parent(&normalized)?;
        let final_path = if parent == "/" {
            format!("/{}", name)
        } else {
            format!("{}/{}", parent, name)
        };
        if self.entries.contains_key(&final_path) {
            return Err(Error::Exists(final_path));
        }
        self.entries.insert(final_path, Entry::Link(target.to_string()));
        Ok(())
    }

    pub fn read_link(&self, path: &str) -> Result<String, Error> {
        Self::validate(path)?;
        let normalized = Self::normalize(path, &self.cwd);
        let resolved = self.resolve(&normalized, false, false)?;
        match self.entries.get(&resolved) {
            Some(Entry::Link(target)) => Ok(target.clone()),
            _ => Err(Error::InvalidTarget(0)),
        }
    }

    pub fn du(&self, path: &str) -> Result<Vec<(String, u64)>, Error> {
        Self::validate(path)?;
        let normalized = Self::normalize(path, &self.cwd);
        let resolved = self.resolve(&normalized, true, false)?;
        if !self.entries.contains_key(&resolved) {
            return Err(Error::Missing(resolved));
        }

        let paths = self.subtree_paths(&resolved);
        let mut sizes = BTreeMap::new();
        for p in &paths {
            let size = match self.entries.get(p).expect("subtree path must exist") {
                Entry::Dir => 0,
                Entry::File(data) => data.len() as u64,
                Entry::Link(target) => target.len() as u64,
            };
            sizes.insert(p.clone(), size);
        }
        for p in paths.iter().rev() {
            let total: u64 = sizes
                .iter()
                .filter(|(candidate, _)| *candidate != p && Self::parent(candidate) == *p)
                .map(|(_, size)| *size)
                .sum();
            if matches!(self.entries.get(p), Some(Entry::Dir)) {
                sizes.insert(p.clone(), total);
            }
        }
        Ok(sizes.into_iter().collect())
    }
}

impl Default for Fs {
    fn default() -> Self {
        Self::new()
    }
}