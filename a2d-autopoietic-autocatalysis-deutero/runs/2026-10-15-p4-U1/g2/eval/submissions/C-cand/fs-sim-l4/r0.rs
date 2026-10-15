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

#[derive(Debug, Clone, PartialEq, Eq)]
enum Entry {
    Dir,
    File(Vec<u8>),
    Link(String),
}

impl Entry {
    fn kind(&self) -> Kind {
        match self {
            Entry::Dir => Kind::Dir,
            Entry::File(_) => Kind::File,
            Entry::Link(_) => Kind::Link,
        }
    }
}

pub struct Fs {
    entries: BTreeMap<String, Entry>,
    cwd: String,
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
    let mut parts: Vec<&str> = if path.starts_with('/') {
        Vec::new()
    } else {
        base.split('/').filter(|s| !s.is_empty()).collect()
    };

    for part in path.split('/') {
        match part {
            "" | "." => {}
            ".." => {
                parts.pop();
            }
            _ => parts.push(part),
        }
    }

    if parts.is_empty() {
        "/".to_owned()
    } else {
        format!("/{}", parts.join("/"))
    }
}

fn parent_path(path: &str) -> String {
    if path == "/" {
        return "/".to_owned();
    }
    match path.rfind('/') {
        Some(0) => "/".to_owned(),
        Some(i) => path[..i].to_owned(),
        _ => "/".to_owned(),
    }
}

fn basename(path: &str) -> String {
    path.rsplit('/').next().unwrap_or("").to_owned()
}

fn is_descendant_or_same(path: &str, ancestor: &str) -> bool {
    if ancestor == "/" {
        return path.starts_with('/');
    }
    path == ancestor
        || path
            .strip_prefix(ancestor)
            .is_some_and(|rest| rest.starts_with('/'))
}

fn join_components(parts: &[String]) -> String {
    if parts.is_empty() {
        "/".to_owned()
    } else {
        format!("/{}", parts.join("/"))
    }
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

    fn normalized(&self, path: &str) -> Result<String, Error> {
        validate_path(path)?;
        Ok(normalize_from(path, &self.cwd))
    }

    fn resolve(&self, path: &str, follow_final: bool) -> Result<String, Error> {
        let mut pending: Vec<String> = path
            .split('/')
            .filter(|s| !s.is_empty())
            .map(str::to_owned)
            .collect();
        let mut resolved: Vec<String> = Vec::new();
        let mut visited = HashSet::new();
        let mut expansions = 0usize;

        while !pending.is_empty() {
            let component = pending.remove(0);
            resolved.push(component);
            let current = join_components(&resolved);
            let final_component = pending.is_empty();

            let entry = match self.entries.get(&current) {
                Some(e) => e,
                None => return Err(Error::Missing(current)),
            };

            if let Entry::Link(target) = entry {
                if !final_component || follow_final {
                    if visited.contains(&current) {
                        return Err(Error::Loop(current));
                    }
                    if expansions >= 8 {
                        return Err(Error::Depth(current));
                    }
                    visited.insert(current.clone());
                    expansions += 1;

                    let physical_parent = parent_path(&current);
                    let target_path = normalize_from(target, &physical_parent);
                    let mut new_pending: Vec<String> = target_path
                        .split('/')
                        .filter(|s| !s.is_empty())
                        .map(str::to_owned)
                        .collect();
                    new_pending.extend(pending.into_iter());
                    pending = new_pending;
                    resolved.clear();
                }
            }

            if !pending.is_empty() {
                let here = join_components(&resolved);
                let Some(entry) = self.entries.get(&here) else {
                    return Err(Error::Missing(here));
                };
                if !matches!(entry, Entry::Dir | Entry::Link(_)) {
                    return Err(Error::NotDir(here));
                }
            }
        }

        Ok(join_components(&resolved))
    }

    fn resolve_parent(&self, canonical: &str) -> Result<(String, String), Error> {
        if canonical == "/" {
            return Ok(("/".to_owned(), "/".to_owned()));
        }
        let parent = parent_path(canonical);
        let name = basename(canonical);
        let resolved_parent = self.resolve(&parent, true)?;
        match self.entries.get(&resolved_parent) {
            Some(Entry::Dir) => Ok((resolved_parent, name)),
            Some(_) => Err(Error::NotDir(resolved_parent)),
            None => Err(Error::Missing(resolved_parent)),
        }
    }

    fn child_path(parent: &str, name: &str) -> String {
        if parent == "/" {
            format!("/{name}")
        } else {
            format!("{parent}/{name}")
        }
    }

    fn stored_children(&self, path: &str) -> Vec<String> {
        self.entries
            .keys()
            .filter(|candidate| *candidate != path && is_descendant_or_same(candidate, path))
            .filter_map(|candidate| {
                let parent = parent_path(candidate);
                (parent == path).then(|| candidate.clone())
            })
            .collect()
    }

    pub fn mkdir(&mut self, path: &str) -> Result<(), Error> {
        let canonical = self.normalized(path)?;
        let (parent, name) = self.resolve_parent(&canonical)?;
        let destination = Self::child_path(&parent, &name);
        if self.entries.contains_key(&destination) {
            return Err(Error::Exists(destination));
        }
        self.entries.insert(destination, Entry::Dir);
        Ok(())
    }

    pub fn write(&mut self, path: &str, data: &[u8]) -> Result<(), Error> {
        let canonical = self.normalized(path)?;
        let resolved = match self.resolve(&canonical, true) {
            Ok(path) => path,
            Err(Error::Missing(missing)) if missing == canonical => {
                let (parent, name) = self.resolve_parent(&canonical)?;
                Self::child_path(&parent, &name)
            }
            Err(error) => return Err(error),
        };

        match self.entries.get(&resolved) {
            Some(Entry::Dir) => Err(Error::IsDir(resolved)),
            Some(Entry::File(_)) => {
                self.entries
                    .insert(resolved, Entry::File(data.to_vec()));
                Ok(())
            }
            Some(Entry::Link(_)) => Err(Error::Missing(resolved)),
            None => {
                let parent = parent_path(&resolved);
                match self.entries.get(&parent) {
                    Some(Entry::Dir) => {
                        self.entries
                            .insert(resolved, Entry::File(data.to_vec()));
                        Ok(())
                    }
                    Some(_) => Err(Error::NotDir(parent)),
                    None => Err(Error::Missing(parent)),
                }
            }
        }
    }

    pub fn read(&self, path: &str) -> Result<Vec<u8>, Error> {
        let canonical = self.normalized(path)?;
        let resolved = self.resolve(&canonical, true)?;
        match self.entries.get(&resolved) {
            Some(Entry::File(data)) => Ok(data.clone()),
            Some(Entry::Dir) => Err(Error::IsDir(resolved)),
            Some(Entry::Link(_)) => Err(Error::Missing(resolved)),
            None => Err(Error::Missing(resolved)),
        }
    }

    pub fn ls(&self, path: &str) -> Result<Vec<(String, Kind)>, Error> {
        let canonical = self.normalized(path)?;
        let resolved = self.resolve(&canonical, true)?;
        match self.entries.get(&resolved) {
            Some(Entry::Dir) => Ok(self
                .stored_children(&resolved)
                .into_iter()
                .filter_map(|child| {
                    self.entries
                        .get(&child)
                        .map(|entry| (basename(&child), entry.kind()))
                })
                .collect()),
            Some(_) => Err(Error::NotDir(resolved)),
            None => Err(Error::Missing(resolved)),
        }
    }

    pub fn cwd(&self) -> String {
        self.cwd.clone()
    }

    pub fn chdir(&mut self, path: &str) -> Result<(), Error> {
        let canonical = self.normalized(path)?;
        let resolved = self.resolve(&canonical, true)?;
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
        let source = self.normalized(from)?;
        let destination = self.normalized(to)?;
        if source == "/" || destination == "/" {
            return Err(Error::Root);
        }

        let (source_parent, source_name) = self.resolve_parent(&source)?;
        let source_path = Self::child_path(&source_parent, &source_name);
        let Some(source_entry) = self.entries.get(&source_path).cloned() else {
            return Err(Error::Missing(source_path));
        };

        if matches!(source_entry, Entry::Dir) && is_descendant_or_same(&self.cwd, &source_path) {
            return Err(Error::Busy(source_path));
        }

        let (destination_parent, destination_name) = self.resolve_parent(&destination)?;
        let destination_path = Self::child_path(&destination_parent, &destination_name);

        if matches!(self.entries.get(&destination_path), Some(Entry::Dir))
            && is_descendant_or_same(&self.cwd, &destination_path)
        {
            return Err(Error::Busy(destination_path));
        }

        if source_path == destination_path {
            return Ok(());
        }

        if matches!(source_entry, Entry::Dir) && is_descendant_or_same(&destination_path, &source_path) {
            return Err(Error::Cycle(destination_path));
        }

        if self.entries.contains_key(&destination_path) && !overwrite {
            return Err(Error::Exists(destination_path));
        }

        if let Some(existing) = self.entries.get(&destination_path) {
            match (&source_entry, existing) {
                (Entry::Dir, Entry::Dir) => {
                    if !self.stored_children(&destination_path).is_empty() {
                        return Err(Error::NotEmpty(destination_path));
                    }
                }
                (Entry::Dir, _) => return Err(Error::NotDir(destination_path)),
                (_, Entry::Dir) => return Err(Error::IsDir(destination_path)),
                _ => {}
            }
        }

        let moving: Vec<(String, Entry)> = self
            .entries
            .iter()
            .filter(|(path, _)| is_descendant_or_same(path, &source_path))
            .map(|(path, entry)| (path.clone(), entry.clone()))
            .collect();

        if self.entries.contains_key(&destination_path) {
            let to_remove: Vec<String> = self
                .entries
                .keys()
                .filter(|path| is_descendant_or_same(path, &destination_path))
                .cloned()
                .collect();
            for path in to_remove {
                self.entries.remove(&path);
            }
        }

        for (old_path, _) in &moving {
            self.entries.remove(old_path);
        }

        for (old_path, entry) in moving {
            let suffix = &old_path[source_path.len()..];
            let new_path = format!("{destination_path}{suffix}");
            self.entries.insert(new_path, entry);
        }

        Ok(())
    }

    pub fn remove(&mut self, path: &str, recursive: bool) -> Result<(), Error> {
        let canonical = self.normalized(path)?;
        if canonical == "/" {
            return Err(Error::Root);
        }

        let (parent, name) = self.resolve_parent(&canonical)?;
        let resolved = Self::child_path(&parent, &name);
        let Some(entry) = self.entries.get(&resolved) else {
            return Err(Error::Missing(resolved));
        };

        if matches!(entry, Entry::Dir) && is_descendant_or_same(&self.cwd, &resolved) {
            return Err(Error::Busy(resolved));
        }

        if matches!(entry, Entry::Dir) && !recursive && !self.stored_children(&resolved).is_empty() {
            return Err(Error::NotEmpty(resolved));
        }

        let removals: Vec<String> = self
            .entries
            .keys()
            .filter(|candidate| is_descendant_or_same(candidate, &resolved))
            .cloned()
            .collect();
        for candidate in removals {
            self.entries.remove(&candidate);
        }
        Ok(())
    }

    pub fn symlink(&mut self, target: &str, link: &str) -> Result<(), Error> {
        let canonical_link = self.normalized(link)?;
        if target.is_empty() {
            return Err(Error::InvalidTarget(0));
        }
        if let Some(i) = target.as_bytes().iter().position(|&b| b == 0) {
            return Err(Error::InvalidTarget(i));
        }

        let (parent, name) = self.resolve_parent(&canonical_link)?;
        let resolved = Self::child_path(&parent, &name);
        if self.entries.contains_key(&resolved) {
            return Err(Error::Exists(resolved));
        }
        self.entries
            .insert(resolved, Entry::Link(target.to_owned()));
        Ok(())
    }

    pub fn read_link(&self, path: &str) -> Result<String, Error> {
        let canonical = self.normalized(path)?;
        if canonical == "/" {
            return Err(Error::InvalidTarget(0));
        }

        let (parent, name) = self.resolve_parent(&canonical)?;
        let resolved = Self::child_path(&parent, &name);
        match self.entries.get(&resolved) {
            Some(Entry::Link(target)) => Ok(target.clone()),
            Some(_) => Err(Error::InvalidTarget(0)),
            None => Err(Error::Missing(resolved)),
        }
    }

    pub fn du(&self, path: &str) -> Result<Vec<(String, u64)>, Error> {
        let canonical = self.normalized(path)?;
        let resolved = self.resolve(&canonical, true)?;
        let Some(entry) = self.entries.get(&resolved) else {
            return Err(Error::Missing(resolved));
        };

        let mut paths: Vec<String> = self
            .entries
            .keys()
            .filter(|candidate| is_descendant_or_same(candidate, &resolved))
            .cloned()
            .collect();
        paths.sort();

        let mut sizes = BTreeMap::new();
        for candidate in &paths {
            let size = match self.entries.get(candidate) {
                Some(Entry::File(data)) => data.len() as u64,
                Some(Entry::Link(target)) => target.len() as u64,
                Some(Entry::Dir) => 0,
                None => 0,
            };
            sizes.insert(candidate.clone(), size);
        }

        for candidate in paths.iter().rev() {
            if matches!(self.entries.get(candidate), Some(Entry::Dir)) {
                let sum = paths
                    .iter()
                    .filter(|child| child.as_str() != candidate.as_str() && is_descendant_or_same(child, candidate))
                    .filter(|child| parent_path(child) == *candidate)
                    .filter_map(|child| sizes.get(child).copied())
                    .fold(0u64, u64::saturating_add);
                sizes.insert(candidate.clone(), sum);
            }
        }

        let _ = entry;
        Ok(paths
            .into_iter()
            .map(|candidate| {
                let size = sizes.get(&candidate).copied().unwrap_or(0);
                (candidate, size)
            })
            .collect())
    }
}

fn main() {}