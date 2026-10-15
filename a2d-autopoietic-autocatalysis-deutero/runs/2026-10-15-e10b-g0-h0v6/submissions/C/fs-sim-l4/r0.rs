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
        path.split('/')
            .filter(|part| !part.is_empty() && *part != ".")
            .fold(Vec::new(), |mut stack, part| {
                if part == ".." {
                    stack.pop();
                } else {
                    stack.push(part.to_string());
                }
                stack
            })
    }

    fn path_from_components(parts: &[String]) -> String {
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
        for part in path.split('/') {
            if part.is_empty() || part == "." {
                continue;
            }
            if part == ".." {
                parts.pop();
            } else {
                parts.push(part.to_string());
            }
        }
        Self::path_from_components(&parts)
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
            String::new()
        } else {
            path.rsplit('/').next().unwrap_or("").to_string()
        }
    }

    fn join(parent: &str, name: &str) -> String {
        if parent == "/" {
            format!("/{}", name)
        } else {
            format!("{}/{}", parent, name)
        }
    }

    fn is_within(path: &str, ancestor: &str) -> bool {
        if ancestor == "/" {
            return path.starts_with('/');
        }
        path == ancestor
            || path
                .strip_prefix(ancestor)
                .is_some_and(|suffix| suffix.starts_with('/'))
    }

    fn kind(entry: &Entry) -> Kind {
        match entry {
            Entry::Dir => Kind::Dir,
            Entry::File(_) => Kind::File,
            Entry::Link(_) => Kind::Link,
        }
    }

    fn resolve(&self, initial: &str, follow_final: bool) -> Result<String, Error> {
        let mut todo = Self::components(initial);
        let mut resolved: Vec<String> = Vec::new();
        let mut visited = HashSet::new();
        let mut expansions = 0usize;

        loop {
            if todo.is_empty() {
                return Ok(Self::path_from_components(&resolved));
            }

            let part = todo.remove(0);
            let candidate_parts: Vec<String> = resolved
                .iter()
                .cloned()
                .chain(std::iter::once(part.clone()))
                .collect();
            let candidate = Self::path_from_components(&candidate_parts);
            let final_component = todo.is_empty();

            match self.entries.get(&candidate) {
                None => return Err(Error::Missing(candidate)),
                Some(Entry::Dir) => resolved.push(part),
                Some(Entry::File(_)) if !final_component => {
                    return Err(Error::NotDir(candidate));
                }
                Some(Entry::File(_)) => resolved.push(part),
                Some(Entry::Link(target)) if follow_final || !final_component => {
                    if visited.contains(&candidate) {
                        return Err(Error::Loop(candidate));
                    }
                    if expansions >= 8 {
                        return Err(Error::Depth(candidate));
                    }
                    visited.insert(candidate.clone());
                    expansions += 1;

                    let link_parent = Self::path_from_components(&resolved);
                    let mut expanded = if target.starts_with('/') {
                        Self::components(target)
                    } else {
                        let mut base = resolved.clone();
                        for target_part in target.split('/') {
                            if target_part.is_empty() || target_part == "." {
                                continue;
                            }
                            if target_part == ".." {
                                base.pop();
                            } else {
                                base.push(target_part.to_string());
                            }
                        }
                        base
                    };
                    let _ = link_parent;
                    expanded.extend(todo.into_iter());
                    todo = expanded;
                    resolved.clear();
                }
                Some(Entry::Link(_)) => resolved.push(part),
            }
        }
    }

    fn resolve_parent(&self, path: &str) -> Result<(String, String), Error> {
        if path == "/" {
            return Ok(("/".to_string(), String::new()));
        }
        let raw_parent = Self::parent(path);
        let name = Self::basename(path);
        let parent = self.resolve(&raw_parent, true)?;
        match self.entries.get(&parent) {
            Some(Entry::Dir) => Ok((parent, name)),
            Some(_) => Err(Error::NotDir(parent)),
            None => Err(Error::Missing(parent)),
        }
    }

    fn children(&self, path: &str) -> Vec<String> {
        self.entries
            .keys()
            .filter(|candidate| {
                *candidate != path
                    && Self::parent(candidate) == path
                    && path != candidate.as_str()
            })
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

    fn subtree_nonempty(&self, path: &str) -> bool {
        self.entries.keys().any(|candidate| {
            candidate.as_str() != path && Self::is_within(candidate, path)
        })
    }

    pub fn mkdir(&mut self, path: &str) -> Result<(), Error> {
        Self::validate(path)?;
        let normalized = Self::normalize(path, &self.cwd);
        if normalized == "/" {
            return Err(Error::Exists("/".to_string()));
        }
        let (parent, name) = self.resolve_parent(&normalized)?;
        let resolved = Self::join(&parent, &name);
        if self.entries.contains_key(&resolved) {
            return Err(Error::Exists(resolved));
        }
        self.entries.insert(resolved, Entry::Dir);
        Ok(())
    }

    pub fn write(&mut self, path: &str, data: &[u8]) -> Result<(), Error> {
        Self::validate(path)?;
        let normalized = Self::normalize(path, &self.cwd);
        let resolved = match self.resolve(&normalized, true) {
            Ok(resolved) => resolved,
            Err(Error::Missing(missing)) if missing == normalized => {
                let (parent, name) = self.resolve_parent(&normalized)?;
                let destination = Self::join(&parent, &name);
                if self.entries.contains_key(&destination) {
                    let final_path = self.resolve(&destination, true)?;
                    match self.entries.get_mut(&final_path) {
                        Some(Entry::File(bytes)) => {
                            *bytes = data.to_vec();
                            return Ok(());
                        }
                        Some(Entry::Dir) => return Err(Error::IsDir(final_path)),
                        Some(Entry::Link(_)) | None => {}
                    }
                }
                self.entries
                    .insert(destination, Entry::File(data.to_vec()));
                return Ok(());
            }
            Err(err) => return Err(err),
        };
        match self.entries.get_mut(&resolved) {
            Some(Entry::File(bytes)) => {
                *bytes = data.to_vec();
                Ok(())
            }
            Some(Entry::Dir) => Err(Error::IsDir(resolved)),
            Some(Entry::Link(_)) | None => Err(Error::Missing(resolved)),
        }
    }

    pub fn read(&self, path: &str) -> Result<Vec<u8>, Error> {
        Self::validate(path)?;
        let normalized = Self::normalize(path, &self.cwd);
        let resolved = self.resolve(&normalized, true)?;
        match self.entries.get(&resolved) {
            Some(Entry::File(bytes)) => Ok(bytes.clone()),
            Some(Entry::Dir) => Err(Error::IsDir(resolved)),
            _ => Err(Error::Missing(resolved)),
        }
    }

    pub fn ls(&self, path: &str) -> Result<Vec<(String, Kind)>, Error> {
        Self::validate(path)?;
        let normalized = Self::normalize(path, &self.cwd);
        let resolved = self.resolve(&normalized, true)?;
        match self.entries.get(&resolved) {
            Some(Entry::Dir) => {}
            Some(_) => return Err(Error::NotDir(resolved)),
            None => return Err(Error::Missing(resolved)),
        }
        let mut result: Vec<(String, Kind)> = self
            .children(&resolved)
            .into_iter()
            .filter_map(|child| {
                self.entries
                    .get(&child)
                    .map(|entry| (Self::basename(&child), Self::kind(entry)))
            })
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
        let resolved = self.resolve(&normalized, true)?;
        match self.entries.get(&resolved) {
            Some(Entry::Dir) => {
                self.cwd = resolved;
                Ok(())
            }
            _ => Err(Error::NotDir(resolved)),
        }
    }

    pub fn rename(&mut self, from: &str, to: &str, overwrite: bool) -> Result<(), Error> {
        Self::validate(from)?;
        let from_path = Self::normalize(from, &self.cwd);
        Self::validate(to)?;
        let to_path = Self::normalize(to, &self.cwd);

        if from_path == "/" || to_path == "/" {
            return Err(Error::Root);
        }

        let (src_parent, src_name) = self.resolve_parent(&from_path)?;
        let source = Self::join(&src_parent, &src_name);
        let source_entry = self
            .entries
            .get(&source)
            .cloned()
            .ok_or_else(|| Error::Missing(source.clone()))?;

        if matches!(source_entry, Entry::Dir) && Self::is_within(&self.cwd, &source) {
            return Err(Error::Busy(source));
        }

        let (dst_parent, dst_name) = self.resolve_parent(&to_path)?;
        let destination = Self::join(&dst_parent, &dst_name);

        if matches!(self.entries.get(&destination), Some(Entry::Dir))
            && Self::is_within(&self.cwd, &destination)
        {
            return Err(Error::Busy(destination));
        }

        if source == destination {
            return Ok(());
        }

        if matches!(source_entry, Entry::Dir) && Self::is_within(&destination, &source) {
            return Err(Error::Cycle(destination));
        }

        let destination_entry = self.entries.get(&destination).cloned();
        if destination_entry.is_some() && !overwrite {
            return Err(Error::Exists(destination));
        }

        if let Some(dest_entry) = &destination_entry {
            match (&source_entry, dest_entry) {
                (Entry::Dir, Entry::Dir) => {
                    if self.subtree_nonempty(&destination) {
                        return Err(Error::NotEmpty(destination));
                    }
                }
                (Entry::Dir, _) => return Err(Error::NotDir(destination)),
                (_, Entry::Dir) => return Err(Error::IsDir(destination)),
                _ => {}
            }
        }

        let moving = self.subtree_paths(&source);
        let mut moved_entries = Vec::new();
        for old in &moving {
            let suffix = &old[source.len()..];
            moved_entries.push((format!("{}{}", destination, suffix), self.entries[old].clone()));
        }

        if destination_entry.is_some() {
            for old in self.subtree_paths(&destination) {
                self.entries.remove(&old);
            }
        }
        for old in moving {
            self.entries.remove(&old);
        }
        for (new_path, entry) in moved_entries {
            self.entries.insert(new_path, entry);
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
        let resolved = Self::join(&parent, &name);
        let entry = self
            .entries
            .get(&resolved)
            .cloned()
            .ok_or_else(|| Error::Missing(resolved.clone()))?;

        if matches!(entry, Entry::Dir) && Self::is_within(&self.cwd, &resolved) {
            return Err(Error::Busy(resolved));
        }
        if matches!(entry, Entry::Dir) && !recursive && self.subtree_nonempty(&resolved) {
            return Err(Error::NotEmpty(resolved));
        }

        for candidate in self.subtree_paths(&resolved) {
            self.entries.remove(&candidate);
        }
        Ok(())
    }

    pub fn symlink(&mut self, target: &str, link: &str) -> Result<(), Error> {
        Self::validate(link)?;
        let normalized = Self::normalize(link, &self.cwd);
        if target.is_empty() {
            return Err(Error::InvalidTarget(0));
        }
        if let Some(i) = target.as_bytes().iter().position(|&b| b == 0) {
            return Err(Error::InvalidTarget(i));
        }

        if normalized == "/" {
            return Err(Error::Exists("/".to_string()));
        }
        let (parent, name) = self.resolve_parent(&normalized)?;
        let resolved = Self::join(&parent, &name);
        if self.entries.contains_key(&resolved) {
            return Err(Error::Exists(resolved));
        }
        self.entries.insert(resolved, Entry::Link(target.to_string()));
        Ok(())
    }

    pub fn read_link(&self, path: &str) -> Result<String, Error> {
        Self::validate(path)?;
        let normalized = Self::normalize(path, &self.cwd);
        let resolved = self.resolve(&normalized, false)?;
        match self.entries.get(&resolved) {
            Some(Entry::Link(target)) => Ok(target.clone()),
            _ => Err(Error::InvalidTarget(0)),
        }
    }

    pub fn du(&self, path: &str) -> Result<Vec<(String, u64)>, Error> {
        Self::validate(path)?;
        let normalized = Self::normalize(path, &self.cwd);
        let resolved = self.resolve(&normalized, true)?;
        let paths = self.subtree_paths(&resolved);
        let mut rows = Vec::with_capacity(paths.len());

        for candidate in paths {
            let size = match self.entries.get(&candidate) {
                Some(Entry::File(bytes)) => bytes.len() as u64,
                Some(Entry::Link(target)) => target.len() as u64,
                Some(Entry::Dir) => self
                    .subtree_paths(&candidate)
                    .into_iter()
                    .filter_map(|descendant| match self.entries.get(&descendant) {
                        Some(Entry::File(bytes)) => Some(bytes.len() as u64),
                        Some(Entry::Link(target)) => Some(target.len() as u64),
                        _ => None,
                    })
                    .sum(),
                None => 0,
            };
            rows.push((candidate, size));
        }
        rows.sort_by(|a, b| a.0.cmp(&b.0));
        Ok(rows)
    }
}

fn main() {}