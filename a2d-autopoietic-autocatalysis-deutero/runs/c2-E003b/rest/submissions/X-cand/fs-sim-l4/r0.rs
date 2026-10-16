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

    pub fn mkdir(&mut self, path: &str) -> Result<(), Error> {
        let normalized = normalize_path(path, &self.cwd)?;
        let (parent, name) = split_parent(&normalized);
        let parent = self.resolve_full(&parent)?;
        self.require_dir(&parent)?;
        let destination = join_path(&parent, &name);
        if self.entries.contains_key(&destination) {
            return Err(Error::Exists(destination));
        }
        self.entries.insert(destination, Entry::Dir);
        Ok(())
    }

    pub fn write(&mut self, path: &str, data: &[u8]) -> Result<(), Error> {
        let normalized = normalize_path(path, &self.cwd)?;
        let resolved = self.resolve_full_allow_missing_final(&normalized)?;
        match self.entries.get(&resolved) {
            Some(Entry::Dir) => return Err(Error::IsDir(resolved)),
            Some(Entry::File(_)) => {}
            Some(Entry::Link(_)) => unreachable!(),
            None => {
                let (parent, _) = split_parent(&resolved);
                self.require_dir(&parent)?;
            }
        }
        self.entries
            .insert(resolved, Entry::File(data.to_vec()));
        Ok(())
    }

    pub fn read(&self, path: &str) -> Result<Vec<u8>, Error> {
        let normalized = normalize_path(path, &self.cwd)?;
        let resolved = self.resolve_full(&normalized)?;
        match self.entries.get(&resolved) {
            Some(Entry::File(data)) => Ok(data.clone()),
            Some(Entry::Dir) => Err(Error::IsDir(resolved)),
            _ => Err(Error::Missing(resolved)),
        }
    }

    pub fn ls(&self, path: &str) -> Result<Vec<(String, Kind)>, Error> {
        let normalized = normalize_path(path, &self.cwd)?;
        let resolved = self.resolve_full(&normalized)?;
        self.require_dir(&resolved)?;
        let prefix = if resolved == "/" {
            "/".to_string()
        } else {
            format!("{}/", resolved)
        };
        let mut result = Vec::new();
        for (entry_path, entry) in &self.entries {
            if entry_path == &resolved || !entry_path.starts_with(&prefix) {
                continue;
            }
            let rest = &entry_path[prefix.len()..];
            if !rest.is_empty() && !rest.contains('/') {
                result.push((rest.to_string(), entry.kind()));
            }
        }
        result.sort_by(|a, b| a.0.cmp(&b.0));
        Ok(result)
    }

    pub fn cwd(&self) -> String {
        self.cwd.clone()
    }

    pub fn chdir(&mut self, path: &str) -> Result<(), Error> {
        let normalized = normalize_path(path, &self.cwd)?;
        let resolved = self.resolve_full(&normalized)?;
        self.require_dir(&resolved)?;
        self.cwd = resolved;
        Ok(())
    }

    pub fn rename(&mut self, from: &str, to: &str, overwrite: bool) -> Result<(), Error> {
        let from = normalize_path(from, &self.cwd)?;
        let to = normalize_path(to, &self.cwd)?;
        if from == "/" || to == "/" {
            return Err(Error::Root);
        }

        let (from_parent, from_name) = split_parent(&from);
        let from_parent = self.resolve_full(&from_parent)?;
        self.require_dir(&from_parent)?;
        let source = join_path(&from_parent, &from_name);
        let source_entry = self
            .entries
            .get(&source)
            .cloned()
            .ok_or_else(|| Error::Missing(source.clone()))?;

        if matches!(source_entry, Entry::Dir) && contains_path(&source, &self.cwd) {
            return Err(Error::Busy(source));
        }

        let (to_parent, to_name) = split_parent(&to);
        let to_parent = self.resolve_full(&to_parent)?;
        self.require_dir(&to_parent)?;
        let destination = join_path(&to_parent, &to_name);

        if matches!(self.entries.get(&destination), Some(Entry::Dir))
            && contains_path(&destination, &self.cwd)
        {
            return Err(Error::Busy(destination));
        }
        if source == destination {
            return Ok(());
        }
        if matches!(source_entry, Entry::Dir) && contains_path(&source, &destination) {
            return Err(Error::Cycle(destination));
        }

        let destination_entry = self.entries.get(&destination).cloned();
        if destination_entry.is_some() && !overwrite {
            return Err(Error::Exists(destination));
        }
        if let Some(existing) = &destination_entry {
            match (&source_entry, existing) {
                (Entry::Dir, Entry::Dir) => {
                    if self.has_children(&destination) {
                        return Err(Error::NotEmpty(destination));
                    }
                }
                (Entry::Dir, _) => return Err(Error::NotDir(destination)),
                (_, Entry::Dir) => return Err(Error::IsDir(destination)),
                _ => {}
            }
        }

        let moved = self.collect_subtree(&source);
        if destination_entry.is_some() {
            self.remove_subtree(&destination);
        }
        self.remove_subtree(&source);
        for (old_path, entry) in moved {
            let suffix = if old_path == source {
                String::new()
            } else {
                old_path[source.len()..].to_string()
            };
            let new_path = format!("{}{}", destination, suffix);
            self.entries.insert(new_path, entry);
        }
        Ok(())
    }

    pub fn remove(&mut self, path: &str, recursive: bool) -> Result<(), Error> {
        let normalized = normalize_path(path, &self.cwd)?;
        if normalized == "/" {
            return Err(Error::Root);
        }
        let (parent, name) = split_parent(&normalized);
        let parent = self.resolve_full(&parent)?;
        self.require_dir(&parent)?;
        let resolved = join_path(&parent, &name);
        let entry = self
            .entries
            .get(&resolved)
            .ok_or_else(|| Error::Missing(resolved.clone()))?;

        if matches!(entry, Entry::Dir) && contains_path(&resolved, &self.cwd) {
            return Err(Error::Busy(resolved));
        }
        if matches!(entry, Entry::Dir) && self.has_children(&resolved) && !recursive {
            return Err(Error::NotEmpty(resolved));
        }
        self.remove_subtree(&resolved);
        Ok(())
    }

    pub fn symlink(&mut self, target: &str, link: &str) -> Result<(), Error> {
        let normalized = normalize_path(link, &self.cwd)?;
        validate_target(target)?;
        let (parent, name) = split_parent(&normalized);
        let parent = self.resolve_full(&parent)?;
        self.require_dir(&parent)?;
        let resolved = join_path(&parent, &name);
        if self.entries.contains_key(&resolved) {
            return Err(Error::Exists(resolved));
        }
        self.entries
            .insert(resolved, Entry::Link(target.to_string()));
        Ok(())
    }

    pub fn read_link(&self, path: &str) -> Result<String, Error> {
        let normalized = normalize_path(path, &self.cwd)?;
        let (parent, name) = split_parent(&normalized);
        let parent = self.resolve_full(&parent)?;
        self.require_dir(&parent)?;
        let resolved = join_path(&parent, &name);
        match self.entries.get(&resolved) {
            Some(Entry::Link(target)) => Ok(target.clone()),
            Some(_) => Err(Error::InvalidTarget(0)),
            None => Err(Error::Missing(resolved)),
        }
    }

    pub fn du(&self, path: &str) -> Result<Vec<(String, u64)>, Error> {
        let normalized = normalize_path(path, &self.cwd)?;
        let resolved = self.resolve_full(&normalized)?;
        let entry = self
            .entries
            .get(&resolved)
            .ok_or_else(|| Error::Missing(resolved.clone()))?;
        if !matches!(entry, Entry::Dir) {
            return Ok(vec![(resolved.clone(), self.entry_size(entry))]);
        }

        let mut rows = Vec::new();
        for (entry_path, entry) in &self.entries {
            if entry_path == &resolved || contains_path(&resolved, entry_path) {
                rows.push((entry_path.clone(), self.subtree_size(entry_path)));
            }
        }
        rows.sort_by(|a, b| a.0.cmp(&b.0));
        Ok(rows)
    }
}

impl Default for Fs {
    fn default() -> Self {
        Self::new()
    }
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

impl Fs {
    fn require_dir(&self, path: &str) -> Result<(), Error> {
        match self.entries.get(path) {
            Some(Entry::Dir) => Ok(()),
            Some(_) => Err(Error::NotDir(path.to_string())),
            None => Err(Error::Missing(path.to_string())),
        }
    }

    fn resolve_full(&self, path: &str) -> Result<String, Error> {
        self.resolve(path, false)
    }

    fn resolve_full_allow_missing_final(&self, path: &str) -> Result<String, Error> {
        self.resolve(path, true)
    }

    fn resolve(&self, path: &str, allow_missing_final: bool) -> Result<String, Error> {
        let mut current = String::new();
        let mut pending: Vec<String> = components(path);
        let mut visited = HashSet::new();
        let mut expansions = 0usize;

        while !pending.is_empty() {
            let component = pending.remove(0);
            current = join_path(&current, &component);
            let is_final = pending.is_empty();

            match self.entries.get(&current) {
                None if allow_missing_final && is_final => return Ok(current),
                None => return Err(Error::Missing(current)),
                Some(Entry::Dir) if !is_final => {}
                Some(_) if !is_final => return Err(Error::NotDir(current)),
                Some(Entry::Link(target)) => {
                    if !visited.insert(current.clone()) {
                        return Err(Error::Loop(current));
                    }
                    expansions += 1;
                    if expansions > 8 {
                        return Err(Error::Depth(current));
                    }
                    let parent = split_parent(&current).0;
                    let target_base = if target.starts_with('/') {
                        "/".to_string()
                    } else {
                        parent
                    };
                    let target_path = normalize_path(target, &target_base)
                        .map_err(|_| Error::InvalidTarget(0))?;
                    let mut expanded = components(&target_path);
                    expanded.extend(pending.into_iter());
                    pending = expanded;
                    current.clear();
                }
                Some(_) => {}
            }
        }
        if current.is_empty() {
            current = "/".to_string();
        }
        if !self.entries.contains_key(&current) {
            if allow_missing_final {
                return Ok(current);
            }
            return Err(Error::Missing(current));
        }
        Ok(current)
    }

    fn has_children(&self, path: &str) -> bool {
        self.entries
            .keys()
            .any(|candidate| candidate != path && contains_path(path, candidate))
    }

    fn collect_subtree(&self, path: &str) -> Vec<(String, Entry)> {
        self.entries
            .iter()
            .filter(|(candidate, _)| *candidate == path || contains_path(path, candidate))
            .map(|(candidate, entry)| (candidate.clone(), entry.clone()))
            .collect()
    }

    fn remove_subtree(&mut self, path: &str) {
        let paths: Vec<String> = self
            .entries
            .keys()
            .filter(|candidate| *candidate == path || contains_path(path, candidate))
            .cloned()
            .collect();
        for candidate in paths {
            self.entries.remove(&candidate);
        }
    }

    fn entry_size(&self, entry: &Entry) -> u64 {
        match entry {
            Entry::Dir => 0,
            Entry::File(data) => data.len() as u64,
            Entry::Link(target) => target.len() as u64,
        }
    }

    fn subtree_size(&self, path: &str) -> u64 {
        self.entries
            .iter()
            .filter(|(candidate, _)| *candidate == path || contains_path(path, candidate))
            .map(|(_, entry)| match entry {
                Entry::Dir => 0,
                _ => self.entry_size(entry),
            })
            .sum()
    }
}

fn validate_path(path: &str) -> Result<(), Error> {
    if let Some(index) = path.as_bytes().iter().position(|byte| *byte == 0) {
        return Err(Error::InvalidPath(index));
    }
    if path.is_empty() {
        return Err(Error::InvalidPath(0));
    }
    Ok(())
}

fn validate_target(target: &str) -> Result<(), Error> {
    if target.is_empty() {
        return Err(Error::InvalidTarget(0));
    }
    if let Some(index) = target.as_bytes().iter().position(|byte| *byte == 0) {
        return Err(Error::InvalidTarget(index));
    }
    Ok(())
}

fn normalize_path(path: &str, cwd: &str) -> Result<String, Error> {
    validate_path(path)?;
    let mut parts = if path.starts_with('/') {
        Vec::new()
    } else {
        components(cwd)
    };
    for component in path.split('/') {
        match component {
            "" | "." => {}
            ".." => {
                parts.pop();
            }
            other => parts.push(other.to_string()),
        }
    }
    Ok(path_from_components(&parts))
}

fn components(path: &str) -> Vec<String> {
    path.split('/')
        .filter(|component| !component.is_empty() && *component != ".")
        .map(str::to_string)
        .collect()
}

fn path_from_components(parts: &[String]) -> String {
    if parts.is_empty() {
        "/".to_string()
    } else {
        format!("/{}", parts.join("/"))
    }
}

fn join_path(parent: &str, name: &str) -> String {
    if parent == "/" {
        format!("/{}", name)
    } else {
        format!("{}/{}", parent, name)
    }
}

fn split_parent(path: &str) -> (String, String) {
    if path == "/" {
        return ("/".to_string(), String::new());
    }
    match path.rfind('/') {
        Some(0) => ("/".to_string(), path[1..].to_string()),
        Some(index) => (path[..index].to_string(), path[index + 1..].to_string()),
        None => ("/".to_string(), path.to_string()),
    }
}

fn contains_path(parent: &str, candidate: &str) -> bool {
    if parent == candidate {
        return true;
    }
    if parent == "/" {
        return candidate.starts_with('/') && candidate != "/";
    }
    candidate
        .strip_prefix(parent)
        .is_some_and(|suffix| suffix.starts_with('/'))
}