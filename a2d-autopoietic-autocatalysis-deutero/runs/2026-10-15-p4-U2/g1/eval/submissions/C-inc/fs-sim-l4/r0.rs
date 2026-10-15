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

fn path_parts(path: &str) -> Result<Vec<String>, Error> {
    if let Some(i) = path.as_bytes().iter().position(|&b| b == 0) {
        return Err(Error::InvalidPath(i));
    }
    if path.is_empty() {
        return Err(Error::InvalidPath(0));
    }
    Ok(path
        .split('/')
        .filter(|s| !s.is_empty() && *s != "." && *s != "..")
        .map(str::to_owned)
        .collect())
}

fn canonical(parts: &[String]) -> String {
    if parts.is_empty() {
        "/".to_owned()
    } else {
        format!("/{}", parts.join("/"))
    }
}

fn components(path: &str) -> Vec<String> {
    if path == "/" {
        Vec::new()
    } else {
        path[1..].split('/').map(str::to_owned).collect()
    }
}

fn normalize(path: &str, cwd: &str) -> Result<String, Error> {
    let _ = path_parts(path)?;
    let mut parts = if path.starts_with('/') {
        Vec::new()
    } else {
        components(cwd)
    };
    for part in path.split('/') {
        match part {
            "" | "." => {}
            ".." => {
                parts.pop();
            }
            _ => parts.push(part.to_owned()),
        }
    }
    Ok(canonical(&parts))
}

fn join_parts(base: &[String], parts: &[String]) -> Vec<String> {
    let mut out = base.to_vec();
    for part in parts {
        match part.as_str() {
            "" | "." => {}
            ".." => {
                out.pop();
            }
            _ => out.push(part.clone()),
        }
    }
    out
}

fn parent_path(path: &str) -> String {
    let mut parts = components(path);
    parts.pop();
    canonical(&parts)
}

fn basename(path: &str) -> String {
    components(path).last().cloned().unwrap_or_default()
}

fn is_within(path: &str, dir: &str) -> bool {
    path == dir || (dir == "/" && path.starts_with('/')) || path.starts_with(&format!("{}/", dir))
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

    fn resolve(
        &self,
        start: &str,
        follow_final: bool,
        suffix: &[String],
        allow_missing_final: bool,
    ) -> Result<String, Error> {
        let mut queue = components(start);
        let mut pending_suffix = suffix.to_vec();
        let mut visited = HashSet::new();
        let mut expansions = 0usize;

        loop {
            if !pending_suffix.is_empty() {
                queue.extend(pending_suffix.drain(..));
            }
            let mut resolved: Vec<String> = Vec::new();
            let mut restart = false;

            for (idx, component) in queue.iter().enumerate() {
                let current = canonical(&resolved);
                match self.entries.get(&current) {
                    None => return Err(Error::Missing(current)),
                    Some(Entry::Dir) => {}
                    Some(_) => return Err(Error::NotDir(current)),
                }

                resolved.push(component.clone());
                let candidate = canonical(&resolved);
                let is_final = idx + 1 == queue.len();

                match self.entries.get(&candidate) {
                    Some(Entry::Link(target)) if follow_final || !is_final => {
                        if !visited.insert(candidate.clone()) {
                            return Err(Error::Loop(candidate));
                        }
                        if expansions >= 8 {
                            return Err(Error::Depth(candidate));
                        }
                        expansions += 1;

                        let target_parts = path_parts(target).map_err(|e| match e {
                            Error::InvalidPath(i) => Error::InvalidTarget(i),
                            _ => Error::InvalidTarget(0),
                        })?;
                        let base = if target.starts_with('/') {
                            Vec::new()
                        } else {
                            components(&parent_path(&candidate))
                        };
                        let mut expanded = join_parts(&base, &target_parts);
                        expanded.extend(queue[idx + 1..].iter().cloned());
                        queue = expanded;
                        restart = true;
                        break;
                    }
                    None if allow_missing_final && is_final => return Ok(candidate),
                    None => return Err(Error::Missing(candidate)),
                    _ => {}
                }
            }

            if restart {
                continue;
            }

            let result = canonical(&queue);
            if queue.is_empty() {
                return Ok("/".to_owned());
            }
            if !self.entries.contains_key(&result) && allow_missing_final {
                return Ok(result);
            }
            if !self.entries.contains_key(&result) {
                return Err(Error::Missing(result));
            }
            return Ok(result);
        }
    }

    fn resolve_parent(&self, path: &str) -> Result<(String, String), Error> {
        let parent = parent_path(path);
        let name = basename(path);
        let resolved_parent = self.resolve(&parent, true, &[], false)?;
        match self.entries.get(&resolved_parent) {
            Some(Entry::Dir) => Ok((resolved_parent, name)),
            Some(_) => Err(Error::NotDir(resolved_parent)),
            None => Err(Error::Missing(resolved_parent)),
        }
    }

    fn child_path(parent: &str, name: &str) -> String {
        if parent == "/" {
            format!("/{}", name)
        } else {
            format!("{}/{}", parent, name)
        }
    }

    fn descendants(&self, path: &str) -> Vec<String> {
        self.entries
            .keys()
            .filter(|p| *p != path && is_within(p, path))
            .cloned()
            .collect()
    }

    pub fn mkdir(&mut self, path: &str) -> Result<(), Error> {
        let normalized = normalize(path, &self.cwd)?;
        let (parent, name) = self.resolve_parent(&normalized)?;
        let target = Self::child_path(&parent, &name);
        if self.entries.contains_key(&target) {
            return Err(Error::Exists(target));
        }
        self.entries.insert(target, Entry::Dir);
        Ok(())
    }

    pub fn write(&mut self, path: &str, data: &[u8]) -> Result<(), Error> {
        let normalized = normalize(path, &self.cwd)?;
        let resolved = self.resolve(&normalized, true, &[], true)?;
        if let Some(entry) = self.entries.get(&resolved) {
            match entry {
                Entry::Dir => return Err(Error::IsDir(resolved)),
                Entry::File(_) => {
                    self.entries
                        .insert(resolved, Entry::File(data.to_vec()));
                    return Ok(());
                }
                Entry::Link(_) => unreachable!(),
            }
        }

        let (parent, name) = self.resolve_parent(&resolved)?;
        let target = Self::child_path(&parent, &name);
        self.entries.insert(target, Entry::File(data.to_vec()));
        Ok(())
    }

    pub fn read(&self, path: &str) -> Result<Vec<u8>, Error> {
        let normalized = normalize(path, &self.cwd)?;
        let resolved = self.resolve(&normalized, true, &[], false)?;
        match self.entries.get(&resolved) {
            Some(Entry::File(bytes)) => Ok(bytes.clone()),
            Some(Entry::Dir) => Err(Error::IsDir(resolved)),
            Some(Entry::Link(_)) | None => Err(Error::Missing(resolved)),
        }
    }

    pub fn ls(&self, path: &str) -> Result<Vec<(String, Kind)>, Error> {
        let normalized = normalize(path, &self.cwd)?;
        let resolved = self.resolve(&normalized, true, &[], false)?;
        match self.entries.get(&resolved) {
            Some(Entry::Dir) => {}
            Some(_) => return Err(Error::NotDir(resolved)),
            None => return Err(Error::Missing(resolved)),
        }

        let mut result = Vec::new();
        for (child, entry) in &self.entries {
            if child == &resolved || parent_path(child) != resolved {
                continue;
            }
            let kind = match entry {
                Entry::Dir => Kind::Dir,
                Entry::File(_) => Kind::File,
                Entry::Link(_) => Kind::Link,
            };
            result.push((basename(child), kind));
        }
        result.sort_by(|a, b| a.0.cmp(&b.0));
        Ok(result)
    }

    pub fn cwd(&self) -> String {
        self.cwd.clone()
    }

    pub fn chdir(&mut self, path: &str) -> Result<(), Error> {
        let normalized = normalize(path, &self.cwd)?;
        let resolved = self.resolve(&normalized, true, &[], false)?;
        match self.entries.get(&resolved) {
            Some(Entry::Dir) => {
                self.cwd = resolved;
                Ok(())
            }
            _ => Err(Error::NotDir(resolved)),
        }
    }

    pub fn rename(&mut self, from: &str, to: &str, overwrite: bool) -> Result<(), Error> {
        let source_normalized = normalize(from, &self.cwd)?;
        let destination_normalized = normalize(to, &self.cwd)?;

        if source_normalized == "/" || destination_normalized == "/" {
            return Err(Error::Root);
        }

        let (source_parent, source_name) = self.resolve_parent(&source_normalized)?;
        let source = Self::child_path(&source_parent, &source_name);
        let source_entry = self
            .entries
            .get(&source)
            .cloned()
            .ok_or_else(|| Error::Missing(source.clone()))?;

        if matches!(source_entry, Entry::Dir) && is_within(&self.cwd, &source) {
            return Err(Error::Busy(source));
        }

        let (destination_parent, destination_name) =
            self.resolve_parent(&destination_normalized)?;
        let destination = Self::child_path(&destination_parent, &destination_name);

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

        let existing_destination = self.entries.get(&destination).cloned();
        if existing_destination.is_some() && !overwrite {
            return Err(Error::Exists(destination));
        }

        if let Some(existing) = &existing_destination {
            match (&source_entry, existing) {
                (Entry::Dir, Entry::Dir) => {
                    if !self.descendants(&destination).is_empty() {
                        return Err(Error::NotEmpty(destination));
                    }
                }
                (Entry::Dir, _) => return Err(Error::NotDir(destination)),
                (_, Entry::Dir) => return Err(Error::IsDir(destination)),
                _ => {}
            }
        }

        let mut moving = Vec::new();
        for (path, entry) in &self.entries {
            if path == &source || is_within(path, &source) {
                let suffix = if path == &source {
                    String::new()
                } else {
                    path[source.len()..].to_owned()
                };
                moving.push((path.clone(), suffix, entry.clone()));
            }
        }

        if let Some(existing) = existing_destination {
            if matches!(existing, Entry::Dir) {
                for path in self
                    .entries
                    .keys()
                    .filter(|p| *p == &destination || is_within(p, &destination))
                    .cloned()
                    .collect::<Vec<_>>()
                {
                    self.entries.remove(&path);
                }
            } else {
                self.entries.remove(&destination);
            }
        }

        for (old, _, _) in &moving {
            self.entries.remove(old);
        }
        for (_, suffix, entry) in moving {
            self.entries
                .insert(format!("{}{}", destination, suffix), entry);
        }
        Ok(())
    }

    pub fn remove(&mut self, path: &str, recursive: bool) -> Result<(), Error> {
        let normalized = normalize(path, &self.cwd)?;
        if normalized == "/" {
            return Err(Error::Root);
        }
        let (parent, name) = self.resolve_parent(&normalized)?;
        let target = Self::child_path(&parent, &name);
        let entry = self
            .entries
            .get(&target)
            .cloned()
            .ok_or_else(|| Error::Missing(target.clone()))?;

        if matches!(entry, Entry::Dir) && is_within(&self.cwd, &target) {
            return Err(Error::Busy(target));
        }

        if matches!(entry, Entry::Dir) {
            let descendants = self.descendants(&target);
            if !descendants.is_empty() && !recursive {
                return Err(Error::NotEmpty(target));
            }
            for descendant in descendants {
                self.entries.remove(&descendant);
            }
        }
        self.entries.remove(&target);
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
        let link_path = Self::child_path(&parent, &name);
        if self.entries.contains_key(&link_path) {
            return Err(Error::Exists(link_path));
        }
        self.entries
            .insert(link_path, Entry::Link(target.to_owned()));
        Ok(())
    }

    pub fn read_link(&self, path: &str) -> Result<String, Error> {
        let normalized = normalize(path, &self.cwd)?;
        let parent = parent_path(&normalized);
        let name = basename(&normalized);
        let resolved_parent = self.resolve(&parent, true, &[], false)?;
        let candidate = Self::child_path(&resolved_parent, &name);
        match self.entries.get(&candidate) {
            Some(Entry::Link(target)) => Ok(target.clone()),
            Some(_) => Err(Error::InvalidTarget(0)),
            None => Err(Error::Missing(candidate)),
        }
    }

    pub fn du(&self, path: &str) -> Result<Vec<(String, u64)>, Error> {
        let normalized = normalize(path, &self.cwd)?;
        let resolved = self.resolve(&normalized, true, &[], false)?;
        let root_entry = self
            .entries
            .get(&resolved)
            .ok_or_else(|| Error::Missing(resolved.clone()))?;

        let mut paths: Vec<String> = self
            .entries
            .keys()
            .filter(|p| *p == &resolved || is_within(p, &resolved))
            .cloned()
            .collect();
        paths.sort();

        let mut rows = Vec::with_capacity(paths.len());
        for item in paths {
            let size = match self.entries.get(&item) {
                Some(Entry::File(bytes)) => bytes.len() as u64,
                Some(Entry::Link(target)) => target.len() as u64,
                Some(Entry::Dir) => self
                    .entries
                    .iter()
                    .filter(|(child, _)| *child == &item || is_within(child, &item))
                    .map(|(_, e)| match e {
                        Entry::File(bytes) => bytes.len() as u64,
                        Entry::Link(target) => target.len() as u64,
                        Entry::Dir => 0,
                    })
                    .sum(),
                None => 0,
            };
            rows.push((item, size));
        }

        let _ = root_entry;
        Ok(rows)
    }
}

fn main() {}