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

    fn validate_path(path: &str) -> Result<(), Error> {
        if let Some(i) = path.as_bytes().iter().position(|&b| b == 0) {
            return Err(Error::InvalidPath(i));
        }
        if path.is_empty() {
            return Err(Error::InvalidPath(0));
        }
        Ok(())
    }

    fn normalize_from(path: &str, base: &str) -> String {
        let mut parts: Vec<&str> = Vec::new();
        if !path.starts_with('/') {
            parts.extend(base.split('/').filter(|s| !s.is_empty()));
        }
        for part in path.split('/') {
            match part {
                "" | "." => {}
                ".." => {
                    parts.pop();
                }
                p => parts.push(p),
            }
        }
        if parts.is_empty() {
            "/".to_string()
        } else {
            format!("/{}", parts.join("/"))
        }
    }

    fn normalize(&self, path: &str) -> Result<String, Error> {
        Self::validate_path(path)?;
        Ok(Self::normalize_from(path, &self.cwd))
    }

    fn parent(path: &str) -> (String, String) {
        if path == "/" {
            return ("/".to_string(), String::new());
        }
        let i = path.rfind('/').unwrap_or(0);
        let parent = if i == 0 {
            "/".to_string()
        } else {
            path[..i].to_string()
        };
        (parent, path[i + 1..].to_string())
    }

    fn join(parent: &str, name: &str) -> String {
        if parent == "/" {
            format!("/{}", name)
        } else {
            format!("{}/{}", parent, name)
        }
    }

    fn is_within(path: &str, dir: &str) -> bool {
        dir == "/" || path == dir || path.strip_prefix(dir).is_some_and(|s| s.starts_with('/'))
    }

    fn resolve_path(
        &self,
        path: &str,
        follow_final: bool,
        allow_missing_final: bool,
    ) -> Result<String, Error> {
        let mut current = path.to_string();
        let mut visited = HashSet::new();
        let mut expansions = 0usize;

        loop {
            let components: Vec<&str> = current.split('/').filter(|s| !s.is_empty()).collect();
            let mut physical = String::from("/");
            let mut restarted = false;

            for (idx, component) in components.iter().enumerate() {
                let next = Self::join(&physical, component);
                let final_component = idx + 1 == components.len();
                let entry = match self.entries.get(&next) {
                    Some(e) => e,
                    None => {
                        if final_component && allow_missing_final {
                            return Ok(next);
                        }
                        return Err(Error::Missing(next));
                    }
                };

                let should_follow = matches!(entry, Entry::Link(_))
                    && (!final_component || follow_final);
                if should_follow {
                    if !visited.insert(next.clone()) {
                        return Err(Error::Loop(next));
                    }
                    if expansions >= 8 {
                        return Err(Error::Depth(next));
                    }
                    expansions += 1;

                    let target = match entry {
                        Entry::Link(t) => t,
                        _ => unreachable!(),
                    };
                    let base = Self::parent(&next).0;
                    let target_path = Self::normalize_from(target, &base);
                    let suffix = &components[idx + 1..];
                    let mut combined = target_path;
                    for part in suffix {
                        combined = Self::join(&combined, part);
                    }
                    current = Self::normalize_from(&combined, "/");
                    restarted = true;
                    break;
                }

                if !final_component && !matches!(entry, Entry::Dir) {
                    return Err(Error::NotDir(next));
                }
                physical = next;
            }

            if !restarted {
                return Ok(physical);
            }
        }
    }

    fn resolve_parent(&self, path: &str) -> Result<(String, String), Error> {
        let (parent, name) = Self::parent(path);
        let resolved_parent = self.resolve_path(&parent, true, false)?;
        match self.entries.get(&resolved_parent) {
            Some(Entry::Dir) => Ok((resolved_parent, name)),
            Some(_) => Err(Error::NotDir(resolved_parent)),
            None => Err(Error::Missing(resolved_parent)),
        }
    }

    fn kind(entry: &Entry) -> Kind {
        match entry {
            Entry::Dir => Kind::Dir,
            Entry::File(_) => Kind::File,
            Entry::Link(_) => Kind::Link,
        }
    }

    fn direct_children(&self, dir: &str) -> Vec<String> {
        self.entries
            .keys()
            .filter(|p| {
                if *p == dir || **p == "/" {
                    return false;
                }
                Self::parent(p).0 == dir
            })
            .cloned()
            .collect()
    }

    fn subtree_paths(&self, root: &str) -> Vec<String> {
        self.entries
            .keys()
            .filter(|p| Self::is_within(p, root))
            .cloned()
            .collect()
    }

    fn directory_size(&self, path: &str) -> u64 {
        self.subtree_paths(path)
            .iter()
            .filter_map(|p| match self.entries.get(p) {
                Some(Entry::File(data)) => Some(data.len() as u64),
                Some(Entry::Link(target)) => Some(target.len() as u64),
                _ => None,
            })
            .sum()
    }

    pub fn mkdir(&mut self, path: &str) -> Result<(), Error> {
        let normalized = self.normalize(path)?;
        let (parent, name) = self.resolve_parent(&normalized)?;
        let destination = Self::join(&parent, &name);
        if self.entries.contains_key(&destination) {
            return Err(Error::Exists(destination));
        }
        self.entries.insert(destination, Entry::Dir);
        Ok(())
    }

    pub fn write(&mut self, path: &str, data: &[u8]) -> Result<(), Error> {
        let normalized = self.normalize(path)?;
        let resolved = self.resolve_path(&normalized, true, true)?;
        if let Some(entry) = self.entries.get(&resolved) {
            if matches!(entry, Entry::Dir) {
                return Err(Error::IsDir(resolved));
            }
        }

        let (parent, name) = Self::parent(&resolved);
        let resolved_parent = self.resolve_path(&parent, true, false)?;
        match self.entries.get(&resolved_parent) {
            Some(Entry::Dir) => {}
            Some(_) => return Err(Error::NotDir(resolved_parent)),
            None => return Err(Error::Missing(resolved_parent)),
        }
        let final_path = Self::join(&resolved_parent, &name);
        if let Some(entry) = self.entries.get(&final_path) {
            if matches!(entry, Entry::Dir) {
                return Err(Error::IsDir(final_path));
            }
        }
        self.entries
            .insert(final_path, Entry::File(data.to_vec()));
        Ok(())
    }

    pub fn read(&self, path: &str) -> Result<Vec<u8>, Error> {
        let normalized = self.normalize(path)?;
        let resolved = self.resolve_path(&normalized, true, false)?;
        match self.entries.get(&resolved) {
            Some(Entry::File(data)) => Ok(data.clone()),
            Some(Entry::Dir) => Err(Error::IsDir(resolved)),
            Some(Entry::Link(_)) => unreachable!(),
            None => Err(Error::Missing(resolved)),
        }
    }

    pub fn ls(&self, path: &str) -> Result<Vec<(String, Kind)>, Error> {
        let normalized = self.normalize(path)?;
        let resolved = self.resolve_path(&normalized, true, false)?;
        match self.entries.get(&resolved) {
            Some(Entry::Dir) => {}
            Some(_) => return Err(Error::NotDir(resolved)),
            None => return Err(Error::Missing(resolved)),
        }
        let mut children = self.direct_children(&resolved);
        children.sort();
        Ok(children
            .into_iter()
            .filter_map(|p| {
                let name = Self::parent(&p).1;
                self.entries.get(&p).map(|e| (name, Self::kind(e)))
            })
            .collect())
    }

    pub fn cwd(&self) -> String {
        self.cwd.clone()
    }

    pub fn chdir(&mut self, path: &str) -> Result<(), Error> {
        let normalized = self.normalize(path)?;
        let resolved = self.resolve_path(&normalized, true, false)?;
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

        let (src_parent, src_name) = Self::resolve_parent(&self, &source)?;
        let src = Self::join(&src_parent, &src_name);
        let src_entry = match self.entries.get(&src) {
            Some(e) => e.clone(),
            None => return Err(Error::Missing(src)),
        };

        if matches!(src_entry, Entry::Dir) && Self::is_within(&self.cwd, &src) {
            return Err(Error::Busy(src));
        }

        let (dst_parent, dst_name) = self.resolve_parent(&destination)?;
        let dst = Self::join(&dst_parent, &dst_name);

        if matches!(self.entries.get(&dst), Some(Entry::Dir))
            && Self::is_within(&self.cwd, &dst)
        {
            return Err(Error::Busy(dst));
        }

        if src == dst {
            return Ok(());
        }

        if matches!(src_entry, Entry::Dir) && Self::is_within(&dst, &src) {
            return Err(Error::Cycle(dst));
        }

        let dst_entry = self.entries.get(&dst).cloned();
        if dst_entry.is_some() && !overwrite {
            return Err(Error::Exists(dst));
        }

        if let Some(ref de) = dst_entry {
            match (&src_entry, de) {
                (Entry::Dir, Entry::Dir) => {
                    if !self.direct_children(&dst).is_empty() {
                        return Err(Error::NotEmpty(dst));
                    }
                }
                (Entry::Dir, _) => return Err(Error::NotDir(dst)),
                (_, Entry::Dir) => return Err(Error::IsDir(dst)),
                _ => {}
            }
        }

        let moving = self.subtree_paths(&src);
        let mut moved_entries = Vec::with_capacity(moving.len());
        for old in &moving {
            let suffix = &old[src.len()..];
            let new = format!("{}{}", dst, suffix);
            if let Some(entry) = self.entries.get(old) {
                moved_entries.push((old.clone(), new, entry.clone()));
            }
        }

        if dst_entry.is_some() {
            for old in self.subtree_paths(&dst) {
                self.entries.remove(&old);
            }
        }
        for old in &moving {
            self.entries.remove(old);
        }
        for (_, new, entry) in moved_entries {
            self.entries.insert(new, entry);
        }
        Ok(())
    }

    pub fn remove(&mut self, path: &str, recursive: bool) -> Result<(), Error> {
        let normalized = self.normalize(path)?;
        if normalized == "/" {
            return Err(Error::Root);
        }
        let (parent, name) = self.resolve_parent(&normalized)?;
        let resolved = Self::join(&parent, &name);
        let entry = match self.entries.get(&resolved) {
            Some(e) => e,
            None => return Err(Error::Missing(resolved)),
        };

        if matches!(entry, Entry::Dir) && Self::is_within(&self.cwd, &resolved) {
            return Err(Error::Busy(resolved));
        }

        if matches!(entry, Entry::Dir)
            && !recursive
            && !self.direct_children(&resolved).is_empty()
        {
            return Err(Error::NotEmpty(resolved));
        }

        let paths = if matches!(entry, Entry::Dir) {
            self.subtree_paths(&resolved)
        } else {
            vec![resolved]
        };
        for p in paths {
            self.entries.remove(&p);
        }
        Ok(())
    }

    pub fn symlink(&mut self, target: &str, link: &str) -> Result<(), Error> {
        let normalized_link = self.normalize(link)?;
        if target.is_empty() {
            return Err(Error::InvalidTarget(0));
        }
        if let Some(i) = target.as_bytes().iter().position(|&b| b == 0) {
            return Err(Error::InvalidTarget(i));
        }

        let (parent, name) = self.resolve_parent(&normalized_link)?;
        let destination = Self::join(&parent, &name);
        if self.entries.contains_key(&destination) {
            return Err(Error::Exists(destination));
        }
        self.entries
            .insert(destination, Entry::Link(target.to_string()));
        Ok(())
    }

    pub fn read_link(&self, path: &str) -> Result<String, Error> {
        let normalized = self.normalize(path)?;
        let resolved = self.resolve_path(&normalized, false, false)?;
        match self.entries.get(&resolved) {
            Some(Entry::Link(target)) => Ok(target.clone()),
            Some(_) => Err(Error::InvalidTarget(0)),
            None => Err(Error::Missing(resolved)),
        }
    }

    pub fn du(&self, path: &str) -> Result<Vec<(String, u64)>, Error> {
        let normalized = self.normalize(path)?;
        let resolved = self.resolve_path(&normalized, true, false)?;
        if !self.entries.contains_key(&resolved) {
            return Err(Error::Missing(resolved));
        }

        let mut paths = self.subtree_paths(&resolved);
        paths.sort();
        Ok(paths
            .into_iter()
            .map(|p| {
                let size = match self.entries.get(&p) {
                    Some(Entry::File(data)) => data.len() as u64,
                    Some(Entry::Link(target)) => target.len() as u64,
                    Some(Entry::Dir) => self.directory_size(&p),
                    None => 0,
                };
                (p, size)
            })
            .collect())
    }
}

impl Default for Fs {
    fn default() -> Self {
        Self::new()
    }
}