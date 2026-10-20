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

fn validate_path(path: &str) -> Result<(), Error> {
    if let Some(i) = path.as_bytes().iter().position(|&b| b == 0) {
        return Err(Error::InvalidPath(i));
    }
    if path.is_empty() {
        return Err(Error::InvalidPath(0));
    }
    Ok(())
}

fn components(path: &str) -> Vec<String> {
    let mut out = Vec::new();
    for part in path.split('/') {
        match part {
            "" | "." => {}
            ".." => {
                out.pop();
            }
            _ => out.push(part.to_string()),
        }
    }
    out
}

fn canonical(parts: &[String]) -> String {
    if parts.is_empty() {
        "/".to_string()
    } else {
        format!("/{}", parts.join("/"))
    }
}

fn path_components(path: &str) -> Vec<String> {
    if path == "/" {
        Vec::new()
    } else {
        path[1..].split('/').map(str::to_string).collect()
    }
}

fn is_within(path: &str, dir: &str) -> bool {
    path == dir || dir == "/" || path.starts_with(&(dir.to_string() + "/"))
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

    fn normalize(&self, path: &str) -> Result<String, Error> {
        validate_path(path)?;
        let mut parts = if path.starts_with('/') {
            Vec::new()
        } else {
            path_components(&self.cwd)
        };
        for part in path.split('/') {
            match part {
                "" | "." => {}
                ".." => {
                    parts.pop();
                }
                _ => parts.push(part.to_string()),
            }
        }
        Ok(canonical(&parts))
    }

    fn resolve(&self, input: &str, follow_final: bool, create_final: bool) -> Result<String, Error> {
        let mut todo = path_components(input);
        let mut resolved: Vec<String> = Vec::new();
        let mut visited = BTreeSet::new();
        let mut expansions = 0usize;

        loop {
            if todo.is_empty() {
                return Ok(canonical(&resolved));
            }
            let name = todo.remove(0);
            resolved.push(name);
            let current = canonical(&resolved);
            let is_final = todo.is_empty();

            if !self.entries.contains_key(&current) {
                if is_final && create_final {
                    return Ok(current);
                }
                return Err(Error::Missing(current));
            }

            let entry = self.entries.get(&current).unwrap();
            if let Entry::Link(target) = entry {
                if !is_final || follow_final {
                    if visited.contains(&current) {
                        return Err(Error::Loop(current));
                    }
                    if expansions >= 8 {
                        return Err(Error::Depth(current));
                    }
                    visited.insert(current.clone());
                    expansions += 1;

                    let mut target_parts = if target.starts_with('/') {
                        Vec::new()
                    } else {
                        resolved[..resolved.len() - 1].to_vec()
                    };
                    for part in target.split('/') {
                        match part {
                            "" | "." => {}
                            ".." => {
                                target_parts.pop();
                            }
                            _ => target_parts.push(part.to_string()),
                        }
                    }
                    let mut combined = target_parts;
                    combined.extend(todo.into_iter());
                    todo = combined;
                    resolved.clear();
                    continue;
                }
            }

            if !is_final && !matches!(entry, Entry::Dir) {
                return Err(Error::NotDir(current));
            }
        }
    }

    fn resolve_parent(&self, path: &str) -> Result<(String, String), Error> {
        let parts = path_components(path);
        if parts.is_empty() {
            return Ok(("/".to_string(), "/".to_string()));
        }
        let basename = parts.last().unwrap().clone();
        let parent = canonical(&parts[..parts.len() - 1]);
        let resolved_parent = self.resolve(&parent, true, false)?;
        match self.entries.get(&resolved_parent) {
            Some(Entry::Dir) => {}
            Some(_) => return Err(Error::NotDir(resolved_parent)),
            None => return Err(Error::Missing(resolved_parent)),
        }
        let final_path = if resolved_parent == "/" {
            format!("/{}", basename)
        } else {
            format!("{}/{}", resolved_parent, basename)
        };
        Ok((resolved_parent, final_path))
    }

    fn has_children(&self, path: &str) -> bool {
        self.entries
            .keys()
            .any(|p| p != path && is_within(p, path))
    }

    fn subtree(&self, path: &str) -> Vec<(String, Entry)> {
        self.entries
            .iter()
            .filter(|(p, _)| is_within(p, path))
            .map(|(p, e)| (p.clone(), e.clone()))
            .collect()
    }

    pub fn mkdir(&mut self, path: &str) -> Result<(), Error> {
        let normalized = self.normalize(path)?;
        if normalized == "/" {
            return Err(Error::Exists("/".to_string()));
        }
        let (_, final_path) = self.resolve_parent(&normalized)?;
        if self.entries.contains_key(&final_path) {
            return Err(Error::Exists(final_path));
        }
        self.entries.insert(final_path, Entry::Dir);
        Ok(())
    }

    pub fn write(&mut self, path: &str, data: &[u8]) -> Result<(), Error> {
        let normalized = self.normalize(path)?;
        let resolved = self.resolve(&normalized, true, true)?;
        if resolved == "/" {
            return Err(Error::IsDir("/".to_string()));
        }

        let entry = self.entries.get(&resolved).cloned();
        match entry {
            Some(Entry::Dir) => Err(Error::IsDir(resolved)),
            Some(Entry::File(_)) => {
                self.entries
                    .insert(resolved, Entry::File(data.to_vec()));
                Ok(())
            }
            Some(Entry::Link(_)) => unreachable!(),
            None => {
                let (parent, _) = self.resolve_parent(&resolved)?;
                let _ = parent;
                self.entries
                    .insert(resolved, Entry::File(data.to_vec()));
                Ok(())
            }
        }
    }

    pub fn read(&self, path: &str) -> Result<Vec<u8>, Error> {
        let normalized = self.normalize(path)?;
        let resolved = self.resolve(&normalized, true, false)?;
        match self.entries.get(&resolved) {
            Some(Entry::File(data)) => Ok(data.clone()),
            Some(Entry::Dir) => Err(Error::IsDir(resolved)),
            Some(Entry::Link(_)) => unreachable!(),
            None => Err(Error::Missing(resolved)),
        }
    }

    pub fn ls(&self, path: &str) -> Result<Vec<(String, Kind)>, Error> {
        let normalized = self.normalize(path)?;
        let resolved = self.resolve(&normalized, true, false)?;
        match self.entries.get(&resolved) {
            Some(Entry::Dir) => {}
            Some(_) => return Err(Error::NotDir(resolved)),
            None => return Err(Error::Missing(resolved)),
        }

        let prefix = if resolved == "/" {
            "/".to_string()
        } else {
            format!("{}/", resolved)
        };
        let mut result = Vec::new();
        for (p, entry) in &self.entries {
            if p == &resolved || !p.starts_with(&prefix) {
                continue;
            }
            let rest = &p[prefix.len()..];
            if !rest.contains('/') {
                let kind = match entry {
                    Entry::Dir => Kind::Dir,
                    Entry::File(_) => Kind::File,
                    Entry::Link(_) => Kind::Link,
                };
                result.push((rest.to_string(), kind));
            }
        }
        result.sort_by(|a, b| a.0.cmp(&b.0));
        Ok(result)
    }

    pub fn cwd(&self) -> String {
        self.cwd.clone()
    }

    pub fn chdir(&mut self, path: &str) -> Result<(), Error> {
        let normalized = self.normalize(path)?;
        let resolved = self.resolve(&normalized, true, false)?;
        match self.entries.get(&resolved) {
            Some(Entry::Dir) => {
                self.cwd = resolved;
                Ok(())
            }
            _ => Err(Error::NotDir(resolved)),
        }
    }

    pub fn rename(&mut self, from: &str, to: &str, overwrite: bool) -> Result<(), Error> {
        let source = self.normalize(from)?;
        let destination = self.normalize(to)?;
        if source == "/" || destination == "/" {
            return Err(Error::Root);
        }

        let (_, source_path) = self.resolve_parent(&source)?;
        let source_entry = self
            .entries
            .get(&source_path)
            .cloned()
            .ok_or_else(|| Error::Missing(source_path.clone()))?;

        if matches!(source_entry, Entry::Dir) && is_within(&self.cwd, &source_path) {
            return Err(Error::Busy(source_path));
        }

        let (_, destination_path) = self.resolve_parent(&destination)?;
        if matches!(self.entries.get(&destination_path), Some(Entry::Dir))
            && is_within(&self.cwd, &destination_path)
        {
            return Err(Error::Busy(destination_path));
        }

        if source_path == destination_path {
            return Ok(());
        }

        if matches!(source_entry, Entry::Dir) && is_within(&destination_path, &source_path) {
            return Err(Error::Cycle(destination_path));
        }

        let destination_entry = self.entries.get(&destination_path).cloned();
        if destination_entry.is_some() && !overwrite {
            return Err(Error::Exists(destination_path));
        }

        if let Some(dest_entry) = &destination_entry {
            match (&source_entry, dest_entry) {
                (Entry::Dir, Entry::Dir) => {
                    if self.has_children(&destination_path) {
                        return Err(Error::NotEmpty(destination_path));
                    }
                }
                (Entry::Dir, _) => return Err(Error::NotDir(destination_path)),
                (_, Entry::Dir) => return Err(Error::IsDir(destination_path)),
                _ => {}
            }
        }

        let moving = self.subtree(&source_path);
        if destination_entry.is_some() {
            let remove_dest: Vec<String> = self
                .entries
                .keys()
                .filter(|p| is_within(p, &destination_path))
                .cloned()
                .collect();
            for p in remove_dest {
                self.entries.remove(&p);
            }
        }

        let remove_source: Vec<String> = self
            .entries
            .keys()
            .filter(|p| is_within(p, &source_path))
            .cloned()
            .collect();
        for p in remove_source {
            self.entries.remove(&p);
        }

        for (old, entry) in moving {
            let suffix = if old == source_path {
                String::new()
            } else {
                old[source_path.len()..].to_string()
            };
            self.entries.insert(format!("{}{}", destination_path, suffix), entry);
        }
        Ok(())
    }

    pub fn remove(&mut self, path: &str, recursive: bool) -> Result<(), Error> {
        let normalized = self.normalize(path)?;
        if normalized == "/" {
            return Err(Error::Root);
        }

        let (_, final_path) = self.resolve_parent(&normalized)?;
        let entry = self
            .entries
            .get(&final_path)
            .cloned()
            .ok_or_else(|| Error::Missing(final_path.clone()))?;

        if matches!(entry, Entry::Dir) && is_within(&self.cwd, &final_path) {
            return Err(Error::Busy(final_path));
        }
        if matches!(entry, Entry::Dir) && self.has_children(&final_path) && !recursive {
            return Err(Error::NotEmpty(final_path));
        }

        let remove: Vec<String> = self
            .entries
            .keys()
            .filter(|p| is_within(p, &final_path))
            .cloned()
            .collect();
        for p in remove {
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
        if normalized_link == "/" {
            return Err(Error::Exists("/".to_string()));
        }
        let (_, final_path) = self.resolve_parent(&normalized_link)?;
        if self.entries.contains_key(&final_path) {
            return Err(Error::Exists(final_path));
        }
        self.entries
            .insert(final_path, Entry::Link(target.to_string()));
        Ok(())
    }

    pub fn read_link(&self, path: &str) -> Result<String, Error> {
        let normalized = self.normalize(path)?;
        let resolved = self.resolve(&normalized, false, false)?;
        match self.entries.get(&resolved) {
            Some(Entry::Link(target)) => Ok(target.clone()),
            Some(_) => Err(Error::InvalidTarget(0)),
            None => Err(Error::Missing(resolved)),
        }
    }

    pub fn du(&self, path: &str) -> Result<Vec<(String, u64)>, Error> {
        let normalized = self.normalize(path)?;
        let resolved = self.resolve(&normalized, true, false)?;
        let entry = self
            .entries
            .get(&resolved)
            .ok_or_else(|| Error::Missing(resolved.clone()))?;

        let subtree = self.subtree(&resolved);
        let mut rows = Vec::new();
        for (p, e) in &subtree {
            let size = match e {
                Entry::File(data) => data.len() as u64,
                Entry::Link(target) => target.len() as u64,
                Entry::Dir => subtree
                    .iter()
                    .filter(|(child, _)| child != p && is_within(child, p))
                    .map(|(_, child_entry)| match child_entry {
                        Entry::File(data) => data.len() as u64,
                        Entry::Link(target) => target.len() as u64,
                        Entry::Dir => 0,
                    })
                    .sum(),
            };
            rows.push((p.clone(), size));
        }
        rows.sort_by(|a, b| a.0.cmp(&b.0));

        if !matches!(entry, Entry::Dir) && rows.len() > 1 {
            rows.truncate(1);
        }
        Ok(rows)
    }
}