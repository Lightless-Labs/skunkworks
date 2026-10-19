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

pub struct Fs {
    entries: BTreeMap<String, Entry>,
    cwd: String,
}

fn canonical(parts: &[String]) -> String {
    if parts.is_empty() {
        "/".to_string()
    } else {
        format!("/{}", parts.join("/"))
    }
}

fn path_parts(path: &str) -> Vec<String> {
    if path == "/" {
        Vec::new()
    } else {
        path[1..].split('/').map(str::to_string).collect()
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

fn normalize(path: &str, cwd: &str) -> Result<String, Error> {
    validate_path(path)?;
    let mut parts = if path.starts_with('/') {
        Vec::new()
    } else {
        path_parts(cwd)
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
    Ok(canonical(&parts))
}

fn parent_and_name(path: &str) -> (String, String) {
    let mut parts = path_parts(path);
    let name = parts.pop().unwrap_or_default();
    (canonical(&parts), name)
}

fn join_child(parent: &str, name: &str) -> String {
    if parent == "/" {
        format!("/{}", name)
    } else {
        format!("{}/{}", parent, name)
    }
}

fn is_within(path: &str, directory: &str) -> bool {
    path == directory
        || (directory == "/" && path.starts_with('/'))
        || (path.starts_with(directory)
            && path.as_bytes().get(directory.len()) == Some(&b'/'))
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

    fn resolve(
        &self,
        initial: &str,
        follow_final: bool,
        allow_missing_final: bool,
    ) -> Result<String, Error> {
        let mut pending = path_parts(initial);
        let mut resolved: Vec<String> = Vec::new();
        let mut visited = BTreeSet::new();
        let mut expansions = 0usize;

        loop {
            if pending.is_empty() {
                return Ok(canonical(&resolved));
            }

            let name = pending.remove(0);
            let prefix_parts = {
                let mut v = resolved.clone();
                v.push(name.clone());
                v
            };
            let prefix = canonical(&prefix_parts);
            let final_component = pending.is_empty();

            match self.entries.get(&prefix) {
                None if final_component && allow_missing_final => return Ok(prefix),
                None => return Err(Error::Missing(prefix)),
                Some(entry) => {
                    let should_follow = matches!(entry, Entry::Link(_))
                        && (!final_component || follow_final);
                    if should_follow {
                        if visited.contains(&prefix) {
                            return Err(Error::Loop(prefix));
                        }
                        if expansions >= 8 {
                            return Err(Error::Depth(prefix));
                        }
                        visited.insert(prefix.clone());
                        expansions += 1;

                        let target = match entry {
                            Entry::Link(target) => target,
                            _ => unreachable!(),
                        };
                        let target_path = if target.starts_with('/') {
                            normalize(target, "/")?
                        } else {
                            let link_parent = canonical(&resolved);
                            normalize(target, &link_parent)?
                        };

                        let mut combined = path_parts(&target_path);
                        combined.extend(pending.into_iter());
                        let combined_path = canonical(&combined);
                        pending = path_parts(&combined_path);
                        resolved.clear();
                        continue;
                    }

                    if !final_component && !matches!(entry, Entry::Dir) {
                        return Err(Error::NotDir(prefix));
                    }
                    resolved.push(name);
                }
            }
        }
    }

    fn resolve_parent(&self, path: &str) -> Result<(String, String), Error> {
        let (parent, name) = parent_and_name(path);
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
                if path == "/" {
                    candidate
                        .strip_prefix('/')
                        .is_some_and(|tail| !tail.is_empty() && !tail.contains('/'))
                } else {
                    candidate
                        .strip_prefix(path)
                        .is_some_and(|tail| tail.starts_with('/') && !tail[1..].is_empty() && !tail[1..].contains('/'))
                }
            })
            .cloned()
            .collect()
    }

    pub fn mkdir(&mut self, path: &str) -> Result<(), Error> {
        let normalized = normalize(path, &self.cwd)?;
        let (parent, name) = self.resolve_parent(&normalized)?;
        let destination = join_child(&parent, &name);
        if self.entries.contains_key(&destination) {
            return Err(Error::Exists(destination));
        }
        self.entries.insert(destination, Entry::Dir);
        Ok(())
    }

    pub fn write(&mut self, path: &str, data: &[u8]) -> Result<(), Error> {
        let normalized = normalize(path, &self.cwd)?;
        let destination = self.resolve(&normalized, true, true)?;
        match self.entries.get(&destination) {
            Some(Entry::Dir) => return Err(Error::IsDir(destination)),
            Some(Entry::File(_)) => {
                self.entries
                    .insert(destination, Entry::File(data.to_vec()));
                return Ok(());
            }
            Some(Entry::Link(_)) => unreachable!(),
            None => {}
        }

        let (parent, _) = parent_and_name(&destination);
        let resolved_parent = self.resolve(&parent, true, false)?;
        match self.entries.get(&resolved_parent) {
            Some(Entry::Dir) => {}
            Some(_) => return Err(Error::NotDir(resolved_parent)),
            None => return Err(Error::Missing(resolved_parent)),
        }
        self.entries
            .insert(destination, Entry::File(data.to_vec()));
        Ok(())
    }

    pub fn read(&self, path: &str) -> Result<Vec<u8>, Error> {
        let normalized = normalize(path, &self.cwd)?;
        let resolved = self.resolve(&normalized, true, false)?;
        match self.entries.get(&resolved) {
            Some(Entry::File(data)) => Ok(data.clone()),
            Some(Entry::Dir) => Err(Error::IsDir(resolved)),
            Some(Entry::Link(_)) => unreachable!(),
            None => Err(Error::Missing(resolved)),
        }
    }

    pub fn ls(&self, path: &str) -> Result<Vec<(String, Kind)>, Error> {
        let normalized = normalize(path, &self.cwd)?;
        let resolved = self.resolve(&normalized, true, false)?;
        match self.entries.get(&resolved) {
            Some(Entry::Dir) => {}
            Some(_) => return Err(Error::NotDir(resolved)),
            None => return Err(Error::Missing(resolved)),
        }

        let mut result = Vec::new();
        for child in self.children(&resolved) {
            let name = child.rsplit('/').next().unwrap_or("").to_string();
            let kind = match self.entries.get(&child) {
                Some(Entry::Dir) => Kind::Dir,
                Some(Entry::File(_)) => Kind::File,
                Some(Entry::Link(_)) => Kind::Link,
                None => continue,
            };
            result.push((name, kind));
        }
        result.sort_by(|a, b| a.0.cmp(&b.0));
        Ok(result)
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
        let source_input = normalize(from, &self.cwd)?;
        let destination_input = normalize(to, &self.cwd)?;

        if source_input == "/" || destination_input == "/" {
            return Err(Error::Root);
        }

        let (source_parent, source_name) = self.resolve_parent(&source_input)?;
        let source = join_child(&source_parent, &source_name);
        let source_entry = self
            .entries
            .get(&source)
            .cloned()
            .ok_or_else(|| Error::Missing(source.clone()))?;

        if matches!(source_entry, Entry::Dir) && is_within(&self.cwd, &source) {
            return Err(Error::Busy(source));
        }

        let (destination_parent, destination_name) = self.resolve_parent(&destination_input)?;
        let destination = join_child(&destination_parent, &destination_name);

        if matches!(self.entries.get(&destination), Some(Entry::Dir))
            && is_within(&self.cwd, &destination)
        {
            return Err(Error::Busy(destination));
        }

        if source == destination {
            return Ok(());
        }

        if matches!(source_entry, Entry::Dir) && is_within(&destination, &source) {
            return Err(Error::Cycle(destination));
        }

        let destination_entry = self.entries.get(&destination).cloned();
        if destination_entry.is_some() && !overwrite {
            return Err(Error::Exists(destination));
        }

        if let Some(ref dest_entry) = destination_entry {
            let source_is_dir = matches!(source_entry, Entry::Dir);
            let dest_is_dir = matches!(dest_entry, Entry::Dir);
            match (source_is_dir, dest_is_dir) {
                (true, false) => return Err(Error::NotDir(destination)),
                (false, true) => return Err(Error::IsDir(destination)),
                (true, true) => {
                    if !self.children(&destination).is_empty() {
                        return Err(Error::NotEmpty(destination));
                    }
                }
                _ => {}
            }
        }

        let moved: Vec<(String, Entry)> = self
            .entries
            .iter()
            .filter(|(key, _)| is_within(key, &source))
            .map(|(key, value)| (key.clone(), value.clone()))
            .collect();

        for (key, _) in &moved {
            self.entries.remove(key);
        }
        if destination_entry.is_some() {
            self.entries.remove(&destination);
        }
        for (old_path, entry) in moved {
            let suffix = &old_path[source.len()..];
            let new_path = format!("{}{}", destination, suffix);
            self.entries.insert(new_path, entry);
        }
        Ok(())
    }

    pub fn remove(&mut self, path: &str, recursive: bool) -> Result<(), Error> {
        let normalized = normalize(path, &self.cwd)?;
        if normalized == "/" {
            return Err(Error::Root);
        }

        let (parent, name) = self.resolve_parent(&normalized)?;
        let target = join_child(&parent, &name);
        let entry = self
            .entries
            .get(&target)
            .cloned()
            .ok_or_else(|| Error::Missing(target.clone()))?;

        if matches!(entry, Entry::Dir) && is_within(&self.cwd, &target) {
            return Err(Error::Busy(target));
        }

        if matches!(entry, Entry::Dir) && !recursive && !self.children(&target).is_empty() {
            return Err(Error::NotEmpty(target));
        }

        let removals: Vec<String> = self
            .entries
            .keys()
            .filter(|key| is_within(key, &target))
            .cloned()
            .collect();
        for key in removals {
            self.entries.remove(&key);
        }
        Ok(())
    }

    pub fn symlink(&mut self, target: &str, link: &str) -> Result<(), Error> {
        let normalized_link = normalize(link, &self.cwd)?;
        if target.is_empty() {
            return Err(Error::InvalidTarget(0));
        }
        if let Some(i) = target.as_bytes().iter().position(|&b| b == 0) {
            return Err(Error::InvalidTarget(i));
        }

        let (parent, name) = self.resolve_parent(&normalized_link)?;
        let destination = join_child(&parent, &name);
        if self.entries.contains_key(&destination) {
            return Err(Error::Exists(destination));
        }
        self.entries
            .insert(destination, Entry::Link(target.to_string()));
        Ok(())
    }

    pub fn read_link(&self, path: &str) -> Result<String, Error> {
        let normalized = normalize(path, &self.cwd)?;
        let (parent, name) = parent_and_name(&normalized);
        let physical_parent = self.resolve(&parent, true, false)?;
        let final_path = join_child(&physical_parent, &name);

        match self.entries.get(&final_path) {
            Some(Entry::Link(target)) => Ok(target.clone()),
            Some(_) => Err(Error::InvalidTarget(0)),
            None => Err(Error::Missing(final_path)),
        }
    }

    pub fn du(&self, path: &str) -> Result<Vec<(String, u64)>, Error> {
        let normalized = normalize(path, &self.cwd)?;
        let resolved = self.resolve(&normalized, true, false)?;
        if !self.entries.contains_key(&resolved) {
            return Err(Error::Missing(resolved));
        }

        let paths: Vec<String> = self
            .entries
            .keys()
            .filter(|candidate| is_within(candidate, &resolved))
            .cloned()
            .collect();

        let mut sizes = BTreeMap::new();
        for entry_path in &paths {
            let size = match self.entries.get(entry_path) {
                Some(Entry::File(data)) => data.len() as u64,
                Some(Entry::Link(target)) => target.len() as u64,
                Some(Entry::Dir) => 0,
                None => 0,
            };
            sizes.insert(entry_path.clone(), size);
        }

        let mut directory_paths = paths.clone();
        directory_paths.sort_by_key(|p| std::cmp::Reverse(path_parts(p).len()));
        for directory in directory_paths {
            if matches!(self.entries.get(&directory), Some(Entry::Dir)) {
                let total = self
                    .entries
                    .keys()
                    .filter(|candidate| candidate.as_str() != directory
                        && is_within(candidate, &directory))
                    .filter(|candidate| {
                        let (parent, _) = parent_and_name(candidate);
                        parent == directory
                    })
                    .map(|child| *sizes.get(child).unwrap_or(&0))
                    .sum::<u64>();
                sizes.insert(directory, total);
            }
        }

        Ok(paths
            .into_iter()
            .map(|entry_path| {
                let size = *sizes.get(&entry_path).unwrap_or(&0);
                (entry_path, size)
            })
            .collect())
    }
}

impl Default for Fs {
    fn default() -> Self {
        Self::new()
    }
}