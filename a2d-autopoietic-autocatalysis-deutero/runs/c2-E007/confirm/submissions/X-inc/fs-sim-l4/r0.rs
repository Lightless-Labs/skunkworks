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

#[derive(Debug, Clone, PartialEq, Eq)]
enum Entry {
    Dir,
    File(Vec<u8>),
    Link(String),
}

#[derive(Debug)]
pub struct Fs {
    entries: BTreeMap<String, Entry>,
    cwd: String,
}

fn components(path: &str) -> Vec<String> {
    if path == "/" {
        Vec::new()
    } else {
        path[1..].split('/').map(str::to_owned).collect()
    }
}

fn path_of(parts: &[String]) -> String {
    if parts.is_empty() {
        "/".to_owned()
    } else {
        format!("/{}", parts.join("/"))
    }
}

fn parent_path(path: &str) -> String {
    let mut parts = components(path);
    parts.pop();
    path_of(&parts)
}

fn basename(path: &str) -> String {
    components(path).last().cloned().unwrap_or_default()
}

fn contains_path(parent: &str, child: &str) -> bool {
    parent == "/" || child == parent || child.strip_prefix(parent).is_some_and(|s| s.starts_with('/'))
}

fn normalize(path: &str, cwd: &str) -> Result<String, Error> {
    if let Some(i) = path.as_bytes().iter().position(|b| *b == 0) {
        return Err(Error::InvalidPath(i));
    }
    if path.is_empty() {
        return Err(Error::InvalidPath(0));
    }

    let mut parts = if path.starts_with('/') { Vec::new() } else { components(cwd) };
    for part in path.split('/') {
        match part {
            "" | "." => {}
            ".." => {
                parts.pop();
            }
            _ => parts.push(part.to_owned()),
        }
    }
    Ok(path_of(&parts))
}

impl Fs {
    pub fn new() -> Self {
        let mut entries = BTreeMap::new();
        entries.insert("/".to_owned(), Entry::Dir);
        Self { entries, cwd: "/".to_owned() }
    }

    fn kind(&self, path: &str) -> Option<Kind> {
        self.entries.get(path).map(|entry| match entry {
            Entry::Dir => Kind::Dir,
            Entry::File(_) => Kind::File,
            Entry::Link(_) => Kind::Link,
        })
    }

    fn children(&self, path: &str) -> Vec<String> {
        self.entries.keys()
            .filter(|p| **p != path && parent_path(p) == path)
            .cloned()
            .collect()
    }

    fn resolve(&self, path: &str, follow_final: bool, create_final: bool) -> Result<String, Error> {
        let mut todo = components(path);
        let mut physical: Vec<String> = Vec::new();
        let mut visited = BTreeSet::new();
        let mut expansions = 0usize;

        loop {
            if todo.is_empty() {
                return Ok(path_of(&physical));
            }

            let part = todo.remove(0);
            physical.push(part);
            let candidate = path_of(&physical);
            let final_component = todo.is_empty();

            let Some(entry) = self.entries.get(&candidate) else {
                if final_component && create_final {
                    return Ok(candidate);
                }
                return Err(Error::Missing(candidate));
            };

            if let Entry::Link(target) = entry {
                if !final_component || follow_final {
                    if !visited.insert(candidate.clone()) {
                        return Err(Error::Loop(candidate));
                    }
                    if expansions >= 8 {
                        return Err(Error::Depth(candidate));
                    }
                    expansions += 1;

                    let target_path = normalize(target, &parent_path(&candidate))
                        .map_err(|_| Error::InvalidTarget(0))?;
                    let mut expanded = components(&target_path);
                    expanded.extend(todo);
                    let expanded_path = path_of(&expanded);
                    todo = components(&expanded_path);
                    physical.clear();
                    continue;
                }
            }

            if !final_component && !matches!(entry, Entry::Dir) {
                return Err(Error::NotDir(candidate));
            }
        }
    }

    fn resolve_parent(&self, path: &str) -> Result<(String, String), Error> {
        let parent = parent_path(path);
        let name = basename(path);
        let resolved_parent = self.resolve(&parent, true, false)?;
        match self.entries.get(&resolved_parent) {
            Some(Entry::Dir) => Ok((resolved_parent, name)),
            Some(_) => Err(Error::NotDir(resolved_parent)),
            None => Err(Error::Missing(resolved_parent)),
        }
    }

    fn path_join(parent: &str, name: &str) -> String {
        if parent == "/" { format!("/{name}") } else { format!("{parent}/{name}") }
    }

    pub fn mkdir(&mut self, path: &str) -> Result<(), Error> {
        let normalized = normalize(path, &self.cwd)?;
        let (parent, name) = self.resolve_parent(&normalized)?;
        let target = Self::path_join(&parent, &name);
        if self.entries.contains_key(&target) {
            return Err(Error::Exists(target));
        }
        self.entries.insert(target, Entry::Dir);
        Ok(())
    }

    pub fn write(&mut self, path: &str, data: &[u8]) -> Result<(), Error> {
        let normalized = normalize(path, &self.cwd)?;
        let resolved = self.resolve(&normalized, true, true)?;
        if resolved == "/" {
            return Err(Error::IsDir(resolved));
        }
        if let Some(entry) = self.entries.get_mut(&resolved) {
            match entry {
                Entry::Dir => Err(Error::IsDir(resolved)),
                Entry::File(bytes) => {
                    *bytes = data.to_vec();
                    Ok(())
                }
                Entry::Link(_) => unreachable!(),
            }
        } else {
            let parent = parent_path(&resolved);
            match self.entries.get(&parent) {
                Some(Entry::Dir) => {
                    self.entries.insert(resolved, Entry::File(data.to_vec()));
                    Ok(())
                }
                Some(_) => Err(Error::NotDir(parent)),
                None => Err(Error::Missing(parent)),
            }
        }
    }

    pub fn read(&self, path: &str) -> Result<Vec<u8>, Error> {
        let normalized = normalize(path, &self.cwd)?;
        let resolved = self.resolve(&normalized, true, false)?;
        match self.entries.get(&resolved) {
            Some(Entry::File(bytes)) => Ok(bytes.clone()),
            Some(Entry::Dir) => Err(Error::IsDir(resolved)),
            Some(Entry::Link(_)) => unreachable!(),
            None => Err(Error::Missing(resolved)),
        }
    }

    pub fn ls(&self, path: &str) -> Result<Vec<(String, Kind)>, Error> {
        let normalized = normalize(path, &self.cwd)?;
        let resolved = self.resolve(&normalized, true, false)?;
        match self.entries.get(&resolved) {
            Some(Entry::Dir) => {
                Ok(self.children(&resolved).into_iter()
                    .map(|child| (basename(&child), self.kind(&child).unwrap()))
                    .collect())
            }
            Some(_) => Err(Error::NotDir(resolved)),
            None => Err(Error::Missing(resolved)),
        }
    }

    pub fn cwd(&self) -> String {
        self.cwd.clone()
    }

    pub fn chdir(&mut self, path: &str) -> Result<(), Error> {
        let normalized = normalize(path, &self.cwd)?;
        let resolved = self.resolve(&normalized, true, false)?;
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
        let source = normalize(from, &self.cwd)?;
        let destination = normalize(to, &self.cwd)?;
        if source == "/" || destination == "/" {
            return Err(Error::Root);
        }

        let (source_parent, source_name) = self.resolve_parent(&source)?;
        let source_path = Self::path_join(&source_parent, &source_name);
        let Some(source_entry) = self.entries.get(&source_path).cloned() else {
            return Err(Error::Missing(source_path));
        };

        if matches!(source_entry, Entry::Dir) && contains_path(&source_path, &self.cwd) {
            return Err(Error::Busy(source_path));
        }

        let (dest_parent, dest_name) = self.resolve_parent(&destination)?;
        let dest_path = Self::path_join(&dest_parent, &dest_name);

        if matches!(self.entries.get(&dest_path), Some(Entry::Dir)) && contains_path(&dest_path, &self.cwd) {
            return Err(Error::Busy(dest_path));
        }

        if source_path == dest_path {
            return Ok(());
        }

        if matches!(source_entry, Entry::Dir) && contains_path(&source_path, &dest_path) {
            return Err(Error::Cycle(dest_path));
        }

        let dest_entry = self.entries.get(&dest_path).cloned();
        if dest_entry.is_some() && !overwrite {
            return Err(Error::Exists(dest_path));
        }

        if let Some(ref dest) = dest_entry {
            match (&source_entry, dest) {
                (Entry::Dir, Entry::Dir) => {
                    if !self.children(&dest_path).is_empty() {
                        return Err(Error::NotEmpty(dest_path));
                    }
                }
                (Entry::Dir, _) => return Err(Error::NotDir(dest_path)),
                (_, Entry::Dir) => return Err(Error::IsDir(dest_path)),
                _ => {}
            }
        }

        let moving: Vec<(String, Entry)> = self.entries.iter()
            .filter(|(p, _)| **p == source_path || contains_path(&source_path, p))
            .map(|(p, e)| (p.clone(), e.clone()))
            .collect();

        if dest_entry.is_some() {
            self.entries.remove(&dest_path);
        }
        for (old, _) in &moving {
            self.entries.remove(old);
        }
        for (old, entry) in moving {
            let suffix = &old[source_path.len()..];
            self.entries.insert(format!("{dest_path}{suffix}"), entry);
        }
        Ok(())
    }

    pub fn remove(&mut self, path: &str, recursive: bool) -> Result<(), Error> {
        let normalized = normalize(path, &self.cwd)?;
        if normalized == "/" {
            return Err(Error::Root);
        }

        let (parent, name) = self.resolve_parent(&normalized)?;
        let target = Self::path_join(&parent, &name);
        let Some(entry) = self.entries.get(&target) else {
            return Err(Error::Missing(target));
        };

        if matches!(entry, Entry::Dir) && contains_path(&target, &self.cwd) {
            return Err(Error::Busy(target));
        }

        if matches!(entry, Entry::Dir) {
            let children = self.children(&target);
            if !recursive && !children.is_empty() {
                return Err(Error::NotEmpty(target));
            }
        }

        let to_remove: Vec<String> = self.entries.keys()
            .filter(|p| **p == target || (recursive && contains_path(&target, p)))
            .cloned()
            .collect();
        for item in to_remove {
            self.entries.remove(&item);
        }
        Ok(())
    }

    pub fn symlink(&mut self, target: &str, link: &str) -> Result<(), Error> {
        let normalized_link = normalize(link, &self.cwd)?;
        if target.is_empty() {
            return Err(Error::InvalidTarget(0));
        }
        if let Some(i) = target.as_bytes().iter().position(|b| *b == 0) {
            return Err(Error::InvalidTarget(i));
        }

        let (parent, name) = self.resolve_parent(&normalized_link)?;
        let link_path = Self::path_join(&parent, &name);
        if self.entries.contains_key(&link_path) {
            return Err(Error::Exists(link_path));
        }
        self.entries.insert(link_path, Entry::Link(target.to_owned()));
        Ok(())
    }

    pub fn read_link(&self, path: &str) -> Result<String, Error> {
        let normalized = normalize(path, &self.cwd)?;
        let resolved = self.resolve(&normalized, false, false)?;
        match self.entries.get(&resolved) {
            Some(Entry::Link(target)) => Ok(target.clone()),
            Some(_) => Err(Error::InvalidTarget(0)),
            None => Err(Error::Missing(resolved)),
        }
    }

    pub fn du(&self, path: &str) -> Result<Vec<(String, u64)>, Error> {
        let normalized = normalize(path, &self.cwd)?;
        let resolved = self.resolve(&normalized, true, false)?;
        let Some(entry) = self.entries.get(&resolved) else {
            return Err(Error::Missing(resolved));
        };

        let paths: Vec<String> = self.entries.keys()
            .filter(|p| **p == resolved || contains_path(&resolved, p))
            .cloned()
            .collect();

        let mut sizes = BTreeMap::new();
        for p in paths.iter().rev() {
            let size = match self.entries.get(p).unwrap() {
                Entry::File(bytes) => bytes.len() as u64,
                Entry::Link(target) => target.len() as u64,
                Entry::Dir => self.children(p).iter()
                    .filter_map(|child| sizes.get(child).copied())
                    .sum(),
            };
            sizes.insert(p.clone(), size);
        }

        let _ = entry;
        Ok(sizes.into_iter().collect())
    }
}