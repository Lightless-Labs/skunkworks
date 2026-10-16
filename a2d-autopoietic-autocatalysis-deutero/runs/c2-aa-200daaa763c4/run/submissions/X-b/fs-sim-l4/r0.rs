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

#[derive(Clone)]
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

    fn components(path: &str) -> Vec<String> {
        if path == "/" {
            Vec::new()
        } else {
            path[1..].split('/').map(str::to_string).collect()
        }
    }

    fn path_from_components(parts: &[String]) -> String {
        if parts.is_empty() {
            "/".to_string()
        } else {
            format!("/{}", parts.join("/"))
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

    fn normalize(path: &str, cwd: &str) -> Result<String, Error> {
        Self::validate(path)?;
        let mut parts = if path.starts_with('/') {
            Vec::new()
        } else {
            Self::components(cwd)
        };
        for component in path.split('/') {
            match component {
                "" | "." => {}
                ".." => {
                    parts.pop();
                }
                _ => parts.push(component.to_string()),
            }
        }
        Ok(Self::path_from_components(&parts))
    }

    fn parent(path: &str) -> String {
        let mut parts = Self::components(path);
        parts.pop();
        Self::path_from_components(&parts)
    }

    fn basename(path: &str) -> String {
        Self::components(path).last().cloned().unwrap_or_default()
    }

    fn join(parent: &str, name: &str) -> String {
        if parent == "/" {
            format!("/{name}")
        } else {
            format!("{parent}/{name}")
        }
    }

    fn is_within(path: &str, dir: &str) -> bool {
        path == dir || (dir == "/" && path.starts_with('/')) || path.starts_with(&format!("{dir}/"))
    }

    fn kind(entry: &Entry) -> Kind {
        match entry {
            Entry::Dir => Kind::Dir,
            Entry::File(_) => Kind::File,
            Entry::Link(_) => Kind::Link,
        }
    }

    fn resolve(&self, path: &str, follow_final: bool, create_missing_final: bool) -> Result<String, Error> {
        let mut pending = Self::components(path);
        let mut resolved: Vec<String> = Vec::new();
        let mut visited = HashSet::new();
        let mut expansions = 0usize;

        loop {
            if pending.is_empty() {
                return Ok(Self::path_from_components(&resolved));
            }

            let component = pending.remove(0);
            let candidate_parts = {
                let mut p = resolved.clone();
                p.push(component.clone());
                p
            };
            let candidate = Self::path_from_components(&candidate_parts);
            let final_component = pending.is_empty();

            let current_path = Self::path_from_components(&resolved);
            match self.entries.get(&current_path) {
                Some(Entry::Dir) => {}
                Some(_) => return Err(Error::NotDir(current_path)),
                None => return Err(Error::Missing(current_path)),
            }

            match self.entries.get(&candidate) {
                Some(Entry::Link(target)) if follow_final || !final_component => {
                    if visited.contains(&candidate) {
                        return Err(Error::Loop(candidate));
                    }
                    visited.insert(candidate.clone());
                    if expansions >= 8 {
                        return Err(Error::Depth(candidate));
                    }
                    expansions += 1;

                    let base = if target.starts_with('/') {
                        "/".to_string()
                    } else {
                        current_path
                    };
                    let target_path = Self::normalize(target, &base)
                        .map_err(|e| match e {
                            Error::InvalidPath(i) => Error::InvalidTarget(i),
                            other => other,
                        })?;
                    let mut new_parts = Self::components(&target_path);
                    new_parts.extend(pending);
                    let expanded = Self::path_from_components(&new_parts);
                    pending = Self::components(&expanded);
                    resolved.clear();
                }
                Some(_) => {
                    if final_component {
                        return Ok(candidate);
                    }
                    resolved.push(component);
                }
                None => {
                    if final_component && create_missing_final {
                        return Ok(candidate);
                    }
                    return Err(Error::Missing(candidate));
                }
            }
        }
    }

    fn resolve_parent(&self, path: &str) -> Result<(String, String), Error> {
        let parent = Self::parent(path);
        let name = Self::basename(path);
        let resolved_parent = self.resolve(&parent, true, false)?;
        match self.entries.get(&resolved_parent) {
            Some(Entry::Dir) => Ok((resolved_parent, name)),
            Some(_) => Err(Error::NotDir(resolved_parent)),
            None => Err(Error::Missing(resolved_parent)),
        }
    }

    fn entry_at_parent(&self, path: &str) -> Result<(String, Option<Entry>), Error> {
        let (parent, name) = self.resolve_parent(path)?;
        let resolved = Self::join(&parent, &name);
        Ok((resolved.clone(), self.entries.get(&resolved).cloned()))
    }

    fn descendants(&self, path: &str) -> Vec<String> {
        self.entries
            .keys()
            .filter(|p| *p != path && Self::is_within(p, path))
            .cloned()
            .collect()
    }

    fn subtree_size(&self, path: &str) -> u64 {
        match self.entries.get(path) {
            Some(Entry::File(data)) => data.len() as u64,
            Some(Entry::Link(target)) => target.len() as u64,
            Some(Entry::Dir) => self
                .descendants(path)
                .iter()
                .map(|p| match self.entries.get(p) {
                    Some(Entry::File(data)) => data.len() as u64,
                    Some(Entry::Link(target)) => target.len() as u64,
                    _ => 0,
                })
                .sum(),
            None => 0,
        }
    }

    pub fn mkdir(&mut self, path: &str) -> Result<(), Error> {
        let normalized = Self::normalize(path, &self.cwd)?;
        if normalized == "/" {
            return Err(Error::Exists("/".to_string()));
        }
        let (resolved, existing) = self.entry_at_parent(&normalized)?;
        if existing.is_some() {
            return Err(Error::Exists(resolved));
        }
        self.entries.insert(resolved, Entry::Dir);
        Ok(())
    }

    pub fn write(&mut self, path: &str, data: &[u8]) -> Result<(), Error> {
        let normalized = Self::normalize(path, &self.cwd)?;
        let resolved = self.resolve(&normalized, true, true)?;
        if resolved == "/" {
            return Err(Error::IsDir("/".to_string()));
        }
        match self.entries.get(&resolved) {
            Some(Entry::Dir) => return Err(Error::IsDir(resolved)),
            Some(Entry::File(_)) | None => {}
            Some(Entry::Link(_)) => unreachable!(),
        }
        let parent = Self::parent(&resolved);
        match self.entries.get(&parent) {
            Some(Entry::Dir) => {}
            Some(_) => return Err(Error::NotDir(parent)),
            None => return Err(Error::Missing(parent)),
        }
        self.entries
            .insert(resolved, Entry::File(data.to_vec()));
        Ok(())
    }

    pub fn read(&self, path: &str) -> Result<Vec<u8>, Error> {
        let normalized = Self::normalize(path, &self.cwd)?;
        let resolved = self.resolve(&normalized, true, false)?;
        match self.entries.get(&resolved) {
            Some(Entry::File(data)) => Ok(data.clone()),
            Some(Entry::Dir) => Err(Error::IsDir(resolved)),
            Some(Entry::Link(_)) => unreachable!(),
            None => Err(Error::Missing(resolved)),
        }
    }

    pub fn ls(&self, path: &str) -> Result<Vec<(String, Kind)>, Error> {
        let normalized = Self::normalize(path, &self.cwd)?;
        let resolved = self.resolve(&normalized, true, false)?;
        match self.entries.get(&resolved) {
            Some(Entry::Dir) => {}
            Some(_) => return Err(Error::NotDir(resolved)),
            None => return Err(Error::Missing(resolved)),
        }
        let mut out = Vec::new();
        for (child, entry) in &self.entries {
            if child == &resolved {
                continue;
            }
            if Self::parent(child) == resolved {
                out.push((Self::basename(child), Self::kind(entry)));
            }
        }
        out.sort_by(|a, b| a.0.cmp(&b.0));
        Ok(out)
    }

    pub fn cwd(&self) -> String {
        self.cwd.clone()
    }

    pub fn chdir(&mut self, path: &str) -> Result<(), Error> {
        let normalized = Self::normalize(path, &self.cwd)?;
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
        let source = Self::normalize(from, &self.cwd)?;
        let destination = Self::normalize(to, &self.cwd)?;
        if source == "/" || destination == "/" {
            return Err(Error::Root);
        }

        let (source_path, source_entry) = self.entry_at_parent(&source)?;
        let source_entry = source_entry.ok_or_else(|| Error::Missing(source_path.clone()))?;

        if matches!(source_entry, Entry::Dir) && Self::is_within(&self.cwd, &source_path) {
            return Err(Error::Busy(source_path));
        }

        let (destination_path, destination_entry) = self.entry_at_parent(&destination)?;

        if matches!(destination_entry, Some(Entry::Dir))
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

        if destination_entry.is_some() && !overwrite {
            return Err(Error::Exists(destination_path));
        }

        if let Some(ref dest) = destination_entry {
            match (&source_entry, dest) {
                (Entry::Dir, Entry::Dir) => {
                    if !self.descendants(&destination_path).is_empty() {
                        return Err(Error::NotEmpty(destination_path));
                    }
                }
                (Entry::Dir, _) => return Err(Error::NotDir(destination_path)),
                (_, Entry::Dir) => return Err(Error::IsDir(destination_path)),
                _ => {}
            }
        }

        let moving = self
            .entries
            .iter()
            .filter(|(p, _)| **p == source_path || Self::is_within(p, &source_path))
            .map(|(p, e)| (p.clone(), e.clone()))
            .collect::<Vec<_>>();

        if destination_entry.is_some() {
            let remove = self
                .entries
                .keys()
                .filter(|p| **p == destination_path || Self::is_within(p, &destination_path))
                .cloned()
                .collect::<Vec<_>>();
            for p in remove {
                self.entries.remove(&p);
            }
        }

        for (old, _) in &moving {
            self.entries.remove(old);
        }

        for (old, entry) in moving {
            let suffix = if old == source_path {
                String::new()
            } else {
                old[source_path.len()..].to_string()
            };
            self.entries.insert(format!("{destination_path}{suffix}"), entry);
        }
        Ok(())
    }

    pub fn remove(&mut self, path: &str, recursive: bool) -> Result<(), Error> {
        let normalized = Self::normalize(path, &self.cwd)?;
        if normalized == "/" {
            return Err(Error::Root);
        }
        let (resolved, entry) = self.entry_at_parent(&normalized)?;
        let entry = entry.ok_or_else(|| Error::Missing(resolved.clone()))?;

        if matches!(entry, Entry::Dir) && Self::is_within(&self.cwd, &resolved) {
            return Err(Error::Busy(resolved));
        }
        let descendants = self.descendants(&resolved);
        if matches!(entry, Entry::Dir) && !descendants.is_empty() && !recursive {
            return Err(Error::NotEmpty(resolved));
        }

        for p in descendants {
            self.entries.remove(&p);
        }
        self.entries.remove(&resolved);
        Ok(())
    }

    pub fn symlink(&mut self, target: &str, link: &str) -> Result<(), Error> {
        let normalized_link = Self::normalize(link, &self.cwd)?;
        if target.is_empty() {
            return Err(Error::InvalidTarget(0));
        }
        if let Some(i) = target.as_bytes().iter().position(|&b| b == 0) {
            return Err(Error::InvalidTarget(i));
        }
        if normalized_link == "/" {
            return Err(Error::Exists("/".to_string()));
        }
        let (resolved, existing) = self.entry_at_parent(&normalized_link)?;
        if existing.is_some() {
            return Err(Error::Exists(resolved));
        }
        self.entries
            .insert(resolved, Entry::Link(target.to_string()));
        Ok(())
    }

    pub fn read_link(&self, path: &str) -> Result<String, Error> {
        let normalized = Self::normalize(path, &self.cwd)?;
        let parent = Self::parent(&normalized);
        let name = Self::basename(&normalized);
        let resolved_parent = self.resolve(&parent, true, false)?;
        match self.entries.get(&resolved_parent) {
            Some(Entry::Dir) => {}
            Some(_) => return Err(Error::NotDir(resolved_parent)),
            None => return Err(Error::Missing(resolved_parent)),
        }
        let final_path = Self::join(&resolved_parent, &name);
        match self.entries.get(&final_path) {
            Some(Entry::Link(target)) => Ok(target.clone()),
            Some(_) => Err(Error::InvalidTarget(0)),
            None => Err(Error::Missing(final_path)),
        }
    }

    pub fn du(&self, path: &str) -> Result<Vec<(String, u64)>, Error> {
        let normalized = Self::normalize(path, &self.cwd)?;
        let resolved = self.resolve(&normalized, true, false)?;
        if !self.entries.contains_key(&resolved) {
            return Err(Error::Missing(resolved));
        }

        let mut paths = vec![resolved.clone()];
        paths.extend(self.descendants(&resolved));
        paths.sort();

        Ok(paths
            .into_iter()
            .map(|p| {
                let size = self.subtree_size(&p);
                (p, size)
            })
            .collect())
    }
}