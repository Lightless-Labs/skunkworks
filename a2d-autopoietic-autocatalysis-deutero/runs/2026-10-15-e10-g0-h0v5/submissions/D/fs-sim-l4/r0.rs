use std::collections::{BTreeMap, BTreeSet};

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

impl Default for Fs {
    fn default() -> Self {
        Self::new()
    }
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
        if let Some(i) = path.as_bytes().iter().position(|&b| b == 0) {
            return Err(Error::InvalidPath(i));
        }
        if path.is_empty() {
            return Err(Error::InvalidPath(0));
        }
        Ok(())
    }

    fn components(path: &str) -> Vec<String> {
        if path == "/" {
            Vec::new()
        } else {
            path[1..].split('/').map(str::to_string).collect()
        }
    }

    fn path_of(parts: &[String]) -> String {
        if parts.is_empty() {
            "/".to_string()
        } else {
            format!("/{}", parts.join("/"))
        }
    }

    fn normalize(&self, input: &str) -> Result<String, Error> {
        Self::validate(input)?;
        let mut parts = if input.starts_with('/') {
            Vec::new()
        } else {
            Self::components(&self.cwd)
        };

        for component in input.split('/') {
            match component {
                "" | "." => {}
                ".." => {
                    parts.pop();
                }
                other => parts.push(other.to_string()),
            }
        }
        Ok(Self::path_of(&parts))
    }

    fn parent(path: &str) -> (String, String) {
        if path == "/" {
            return ("/".to_string(), String::new());
        }
        let idx = path.rfind('/').unwrap_or(0);
        let parent = if idx == 0 {
            "/".to_string()
        } else {
            path[..idx].to_string()
        };
        (parent, path[idx + 1..].to_string())
    }

    fn join_parts(base: &str, target: &str) -> Vec<String> {
        let mut parts = if target.starts_with('/') {
            Vec::new()
        } else {
            Self::components(base)
        };
        for component in target.split('/') {
            match component {
                "" | "." => {}
                ".." => {
                    parts.pop();
                }
                other => parts.push(other.to_string()),
            }
        }
        parts
    }

    fn within(path: &str, dir: &str) -> bool {
        if dir == "/" {
            return path.starts_with('/');
        }
        path == dir
            || path
                .strip_prefix(dir)
                .map(|suffix| suffix.starts_with('/'))
                .unwrap_or(false)
    }

    fn kind(entry: &Entry) -> Kind {
        match entry {
            Entry::Dir => Kind::Dir,
            Entry::File(_) => Kind::File,
            Entry::Link(_) => Kind::Link,
        }
    }

    fn resolve(&self, path: &str, follow_final: bool, allow_missing_final: bool) -> Result<String, Error> {
        let original = Self::components(path);
        let mut pending = original;
        let mut resolved: Vec<String> = Vec::new();
        let mut visited = BTreeSet::new();
        let mut expansions = 0usize;
        let mut i = 0usize;

        loop {
            if i >= pending.len() {
                return Ok(Self::path_of(&resolved));
            }

            let component = pending[i].clone();
            resolved.push(component);
            let current = Self::path_of(&resolved);
            let final_component = i + 1 == pending.len();

            match self.entries.get(&current) {
                None => {
                    if final_component && allow_missing_final {
                        return Ok(current);
                    }
                    return Err(Error::Missing(current));
                }
                Some(Entry::Dir) => {}
                Some(Entry::File(_)) => {
                    if !final_component {
                        return Err(Error::NotDir(current));
                    }
                }
                Some(Entry::Link(target)) if follow_final || !final_component => {
                    if visited.contains(&current) {
                        return Err(Error::Loop(current));
                    }
                    if expansions >= 8 {
                        return Err(Error::Depth(current));
                    }
                    visited.insert(current.clone());
                    expansions += 1;

                    let physical_parent = Self::path_of(&resolved[..resolved.len() - 1]);
                    let mut new_parts = Self::join_parts(&physical_parent, target);
                    new_parts.extend(pending[i + 1..].iter().cloned());

                    let mut normalized = Vec::new();
                    for c in new_parts {
                        match c.as_str() {
                            "" | "." => {}
                            ".." => {
                                normalized.pop();
                            }
                            _ => normalized.push(c),
                        }
                    }
                    pending = normalized;
                    resolved.clear();
                    i = 0;
                    continue;
                }
                Some(Entry::Link(_)) => {}
            }

            i += 1;
        }
    }

    fn resolve_parent(&self, path: &str) -> Result<(String, String), Error> {
        let (parent, name) = Self::parent(path);
        let physical_parent = self.resolve(&parent, true, false)?;
        match self.entries.get(&physical_parent) {
            Some(Entry::Dir) => Ok((physical_parent, name)),
            Some(_) => Err(Error::NotDir(physical_parent)),
            None => Err(Error::Missing(physical_parent)),
        }
    }

    fn children(&self, path: &str) -> Vec<String> {
        self.entries
            .keys()
            .filter(|candidate| {
                if candidate.as_str() == path || !Self::within(candidate, path) {
                    return false;
                }
                let (parent, _) = Self::parent(candidate);
                parent == path
            })
            .cloned()
            .collect()
    }

    fn subtree_paths(&self, path: &str) -> Vec<String> {
        self.entries
            .keys()
            .filter(|candidate| Self::within(candidate, path))
            .cloned()
            .collect()
    }

    pub fn mkdir(&mut self, path: &str) -> Result<(), Error> {
        let canonical = self.normalize(path)?;
        if canonical == "/" {
            return Err(Error::Exists(canonical));
        }
        let (parent, name) = self.resolve_parent(&canonical)?;
        let physical = if parent == "/" {
            format!("/{}", name)
        } else {
            format!("{}/{}", parent, name)
        };
        if self.entries.contains_key(&physical) {
            return Err(Error::Exists(physical));
        }
        self.entries.insert(physical, Entry::Dir);
        Ok(())
    }

    pub fn write(&mut self, path: &str, data: &[u8]) -> Result<(), Error> {
        let canonical = self.normalize(path)?;
        let resolved = self.resolve(&canonical, true, true)?;
        if resolved == "/" {
            return Err(Error::IsDir(resolved));
        }

        let (parent, name) = Self::parent(&resolved);
        let physical_parent = self.resolve(&parent, true, false)?;
        match self.entries.get(&physical_parent) {
            Some(Entry::Dir) => {}
            Some(_) => return Err(Error::NotDir(physical_parent)),
            None => return Err(Error::Missing(physical_parent)),
        }
        let physical = if physical_parent == "/" {
            format!("/{}", name)
        } else {
            format!("{}/{}", physical_parent, name)
        };

        match self.entries.get(&physical) {
            Some(Entry::Dir) => Err(Error::IsDir(physical)),
            Some(Entry::File(_)) => {
                self.entries.insert(physical, Entry::File(data.to_vec()));
                Ok(())
            }
            Some(Entry::Link(_)) => unreachable!(),
            None => {
                self.entries.insert(physical, Entry::File(data.to_vec()));
                Ok(())
            }
        }
    }

    pub fn read(&self, path: &str) -> Result<Vec<u8>, Error> {
        let canonical = self.normalize(path)?;
        let resolved = self.resolve(&canonical, true, false)?;
        match self.entries.get(&resolved) {
            Some(Entry::File(data)) => Ok(data.clone()),
            Some(Entry::Dir) => Err(Error::IsDir(resolved)),
            Some(Entry::Link(_)) => unreachable!(),
            None => Err(Error::Missing(resolved)),
        }
    }

    pub fn ls(&self, path: &str) -> Result<Vec<(String, Kind)>, Error> {
        let canonical = self.normalize(path)?;
        let resolved = self.resolve(&canonical, true, false)?;
        match self.entries.get(&resolved) {
            Some(Entry::Dir) => {}
            Some(_) => return Err(Error::NotDir(resolved)),
            None => return Err(Error::Missing(resolved)),
        }

        let mut result = Vec::new();
        for child in self.children(&resolved) {
            let (_, name) = Self::parent(&child);
            if let Some(entry) = self.entries.get(&child) {
                result.push((name, Self::kind(entry)));
            }
        }
        result.sort_by(|a, b| a.0.cmp(&b.0));
        Ok(result)
    }

    pub fn cwd(&self) -> String {
        self.cwd.clone()
    }

    pub fn chdir(&mut self, path: &str) -> Result<(), Error> {
        let canonical = self.normalize(path)?;
        let resolved = self.resolve(&canonical, true, false)?;
        match self.entries.get(&resolved) {
            Some(Entry::Dir) => {
                self.cwd = resolved;
                Ok(())
            }
            Some(_) => Err(Error::NotDir(resolved)),
            None => Err(Error::Missing(resolved)),
        }
    }

    pub fn rename(&mut self, from: &str, to: &str, overwrite: bool) -> Result<(), Error> {
        let source = self.normalize(from)?;
        let destination = self.normalize(to)?;

        if source == "/" || destination == "/" {
            return Err(Error::Root);
        }

        let (src_parent, src_name) = Self::parent(&source);
        let src_physical_parent = self.resolve(&src_parent, true, false)?;
        match self.entries.get(&src_physical_parent) {
            Some(Entry::Dir) => {}
            Some(_) => return Err(Error::NotDir(src_physical_parent)),
            None => return Err(Error::Missing(src_physical_parent)),
        }
        let src = if src_physical_parent == "/" {
            format!("/{}", src_name)
        } else {
            format!("{}/{}", src_physical_parent, src_name)
        };
        let src_entry = self
            .entries
            .get(&src)
            .cloned()
            .ok_or_else(|| Error::Missing(src.clone()))?;

        if matches!(src_entry, Entry::Dir) && Self::within(&self.cwd, &src) {
            return Err(Error::Busy(src));
        }

        let (dst_parent, dst_name) = Self::parent(&destination);
        let dst_physical_parent = self.resolve(&dst_parent, true, false)?;
        match self.entries.get(&dst_physical_parent) {
            Some(Entry::Dir) => {}
            Some(_) => return Err(Error::NotDir(dst_physical_parent)),
            None => return Err(Error::Missing(dst_physical_parent)),
        }
        let dst = if dst_physical_parent == "/" {
            format!("/{}", dst_name)
        } else {
            format!("{}/{}", dst_physical_parent, dst_name)
        };

        if matches!(self.entries.get(&dst), Some(Entry::Dir)) && Self::within(&self.cwd, &dst) {
            return Err(Error::Busy(dst));
        }

        if src == dst {
            return Ok(());
        }

        if matches!(src_entry, Entry::Dir) && Self::within(&dst, &src) {
            return Err(Error::Cycle(dst));
        }

        let dst_entry = self.entries.get(&dst).cloned();
        if dst_entry.is_some() && !overwrite {
            return Err(Error::Exists(dst));
        }

        if let Some(ref de) = dst_entry {
            match (&src_entry, de) {
                (Entry::Dir, Entry::Dir) => {
                    if self.children(&dst).len() > 0 {
                        return Err(Error::NotEmpty(dst));
                    }
                }
                (Entry::Dir, _) => return Err(Error::NotDir(dst)),
                (_, Entry::Dir) => return Err(Error::IsDir(dst)),
                _ => {}
            }
        }

        let source_paths = self.subtree_paths(&src);
        let mut moved = Vec::new();
        for old in source_paths {
            let suffix = &old[src.len()..];
            let new = format!("{}{}", dst, suffix);
            if let Some(entry) = self.entries.get(&old).cloned() {
                moved.push((old, new, entry));
            }
        }

        if dst_entry.is_some() {
            for old in self.subtree_paths(&dst) {
                self.entries.remove(&old);
            }
        }
        for (old, _, _) in &moved {
            self.entries.remove(old);
        }
        for (_, new, entry) in moved {
            self.entries.insert(new, entry);
        }
        Ok(())
    }

    pub fn remove(&mut self, path: &str, recursive: bool) -> Result<(), Error> {
        let canonical = self.normalize(path)?;
        if canonical == "/" {
            return Err(Error::Root);
        }

        let (parent, name) = Self::parent(&canonical);
        let physical_parent = self.resolve(&parent, true, false)?;
        match self.entries.get(&physical_parent) {
            Some(Entry::Dir) => {}
            Some(_) => return Err(Error::NotDir(physical_parent)),
            None => return Err(Error::Missing(physical_parent)),
        }
        let resolved = if physical_parent == "/" {
            format!("/{}", name)
        } else {
            format!("{}/{}", physical_parent, name)
        };

        let entry = self
            .entries
            .get(&resolved)
            .cloned()
            .ok_or_else(|| Error::Missing(resolved.clone()))?;

        if matches!(entry, Entry::Dir) && Self::within(&self.cwd, &resolved) {
            return Err(Error::Busy(resolved));
        }

        if matches!(entry, Entry::Dir) {
            let children = self.children(&resolved);
            if !recursive && !children.is_empty() {
                return Err(Error::NotEmpty(resolved));
            }
        }

        for item in self.subtree_paths(&resolved) {
            self.entries.remove(&item);
        }
        Ok(())
    }

    pub fn symlink(&mut self, target: &str, link: &str) -> Result<(), Error> {
        let canonical_link = self.normalize(link)?;
        if target.is_empty() {
            return Err(Error::InvalidTarget(0));
        }
        if let Some(i) = target.as_bytes().iter().position(|&b| b == 0) {
            return Err(Error::InvalidTarget(i));
        }
        if canonical_link == "/" {
            return Err(Error::Exists(canonical_link));
        }

        let (parent, name) = self.resolve_parent(&canonical_link)?;
        let physical = if parent == "/" {
            format!("/{}", name)
        } else {
            format!("{}/{}", parent, name)
        };
        if self.entries.contains_key(&physical) {
            return Err(Error::Exists(physical));
        }
        self.entries.insert(physical, Entry::Link(target.to_string()));
        Ok(())
    }

    pub fn read_link(&self, path: &str) -> Result<String, Error> {
        let canonical = self.normalize(path)?;
        if canonical == "/" {
            return Err(Error::InvalidTarget(0));
        }
        let (parent, name) = Self::parent(&canonical);
        let physical_parent = self.resolve(&parent, true, false)?;
        match self.entries.get(&physical_parent) {
            Some(Entry::Dir) => {}
            Some(_) => return Err(Error::NotDir(physical_parent)),
            None => return Err(Error::Missing(physical_parent)),
        }
        let physical = if physical_parent == "/" {
            format!("/{}", name)
        } else {
            format!("{}/{}", physical_parent, name)
        };
        match self.entries.get(&physical) {
            Some(Entry::Link(target)) => Ok(target.clone()),
            Some(_) => Err(Error::InvalidTarget(0)),
            None => Err(Error::Missing(physical)),
        }
    }

    pub fn du(&self, path: &str) -> Result<Vec<(String, u64)>, Error> {
        let canonical = self.normalize(path)?;
        let resolved = self.resolve(&canonical, true, false)?;
        let root_entry = self
            .entries
            .get(&resolved)
            .ok_or_else(|| Error::Missing(resolved.clone()))?;

        let paths = match root_entry {
            Entry::Dir => self.subtree_paths(&resolved),
            _ => vec![resolved.clone()],
        };

        let mut sizes = BTreeMap::new();
        for item in paths {
            let size = match self.entries.get(&item) {
                Some(Entry::File(bytes)) => bytes.len() as u64,
                Some(Entry::Link(target)) => target.len() as u64,
                Some(Entry::Dir) => self
                    .subtree_paths(&item)
                    .into_iter()
                    .filter_map(|descendant| {
                        if descendant == item {
                            return None;
                        }
                        match self.entries.get(&descendant) {
                            Some(Entry::File(bytes)) => Some(bytes.len() as u64),
                            Some(Entry::Link(target)) => Some(target.len() as u64),
                            _ => None,
                        }
                    })
                    .sum(),
                None => 0,
            };
            sizes.insert(item, size);
        }

        Ok(sizes.into_iter().collect())
    }
}