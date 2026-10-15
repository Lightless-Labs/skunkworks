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

fn components(path: &str, base: &[String]) -> Result<Vec<String>, Error> {
    if let Some(i) = path.as_bytes().iter().position(|&b| b == 0) {
        return Err(Error::InvalidPath(i));
    }
    if path.is_empty() {
        return Err(Error::InvalidPath(0));
    }

    let mut out = if path.starts_with('/') {
        Vec::new()
    } else {
        base.to_vec()
    };

    for part in path.split('/') {
        match part {
            "" | "." => {}
            ".." => {
                out.pop();
            }
            _ => out.push(part.to_owned()),
        }
    }
    Ok(out)
}

fn from_canonical(path: &str) -> Vec<String> {
    if path == "/" {
        Vec::new()
    } else {
        path[1..].split('/').map(str::to_owned).collect()
    }
}

fn canonical(parts: &[String]) -> String {
    if parts.is_empty() {
        "/".to_owned()
    } else {
        format!("/{}", parts.join("/"))
    }
}

fn joined(parent: &[String], name: &str) -> String {
    let mut p = parent.to_vec();
    p.push(name.to_owned());
    canonical(&p)
}

fn parent_and_name(parts: &[String]) -> (Vec<String>, String) {
    let mut parent = parts.to_vec();
    let name = parent.pop().unwrap_or_default();
    (parent, name)
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

    fn resolve_parts(
        &self,
        input: Vec<String>,
        allow_missing_final: bool,
    ) -> Result<String, Error> {
        let mut pending = input;
        let mut visited = BTreeSet::new();
        let mut expansions = 0usize;

        loop {
            let mut physical: Vec<String> = Vec::new();
            let mut restart = false;

            for i in 0..pending.len() {
                let path = joined(&physical, &pending[i]);
                let Some(entry) = self.entries.get(&path) else {
                    if allow_missing_final && i + 1 == pending.len() {
                        return Ok(path);
                    }
                    return Err(Error::Missing(path));
                };

                if i + 1 < pending.len() && !matches!(entry, Entry::Dir | Entry::Link(_)) {
                    return Err(Error::NotDir(path));
                }

                if let Entry::Link(target) = entry {
                    if !visited.insert(path.clone()) {
                        return Err(Error::Loop(path));
                    }
                    if expansions == 8 {
                        return Err(Error::Depth(path));
                    }
                    expansions += 1;

                    let base = if target.starts_with('/') {
                        Vec::new()
                    } else {
                        physical.clone()
                    };
                    let mut expanded = components(target, &base)?;
                    expanded.extend_from_slice(&pending[i + 1..]);
                    let normalized = canonical(&expanded);
                    pending = from_canonical(&normalized);
                    restart = true;
                    break;
                }

                physical.push(pending[i].clone());
            }

            if restart {
                continue;
            }
            return Ok(canonical(&physical));
        }
    }

    fn resolve_path(&self, input: &str, allow_missing_final: bool) -> Result<String, Error> {
        let base = from_canonical(&self.cwd);
        let parts = components(input, &base)?;
        self.resolve_parts(parts, allow_missing_final)
    }

    fn direct_kind(&self, path: &str) -> Option<Kind> {
        self.entries.get(path).map(|e| match e {
            Entry::Dir => Kind::Dir,
            Entry::File(_) => Kind::File,
            Entry::Link(_) => Kind::Link,
        })
    }

    fn parent_for(&self, parts: &[String]) -> Result<(String, String), Error> {
        let (parent_parts, name) = parent_and_name(parts);
        let parent = self.resolve_parts(parent_parts, false)?;
        match self.entries.get(&parent) {
            Some(Entry::Dir) => Ok((parent, name)),
            Some(_) => Err(Error::NotDir(parent)),
            None => Err(Error::Missing(parent)),
        }
    }

    pub fn mkdir(&mut self, path: &str) -> Result<(), Error> {
        let parts = components(path, &from_canonical(&self.cwd))?;
        if parts.is_empty() {
            return Err(Error::Exists("/".to_owned()));
        }
        let (parent, name) = self.parent_for(&parts)?;
        let destination = joined(&from_canonical(&parent), &name);
        if self.entries.contains_key(&destination) {
            return Err(Error::Exists(destination));
        }
        self.entries.insert(destination, Entry::Dir);
        Ok(())
    }

    pub fn write(&mut self, path: &str, data: &[u8]) -> Result<(), Error> {
        let resolved = self.resolve_path(path, true)?;
        match self.entries.get(&resolved) {
            Some(Entry::Dir) => Err(Error::IsDir(resolved)),
            Some(Entry::File(_)) => {
                self.entries
                    .insert(resolved, Entry::File(data.to_vec()));
                Ok(())
            }
            Some(Entry::Link(_)) => unreachable!(),
            None => {
                let parts = from_canonical(&resolved);
                let (parent, _) = self.parent_for(&parts)?;
                self.entries
                    .insert(resolved, Entry::File(data.to_vec()));
                debug_assert!(self.entries.contains_key(&parent));
                Ok(())
            }
        }
    }

    pub fn read(&self, path: &str) -> Result<Vec<u8>, Error> {
        let resolved = self.resolve_path(path, false)?;
        match self.entries.get(&resolved) {
            Some(Entry::File(data)) => Ok(data.clone()),
            Some(Entry::Dir) => Err(Error::IsDir(resolved)),
            Some(Entry::Link(_)) => unreachable!(),
            None => Err(Error::Missing(resolved)),
        }
    }

    pub fn ls(&self, path: &str) -> Result<Vec<(String, Kind)>, Error> {
        let resolved = self.resolve_path(path, false)?;
        if !matches!(self.entries.get(&resolved), Some(Entry::Dir)) {
            return Err(Error::NotDir(resolved));
        }

        let mut result = Vec::new();
        for (entry_path, entry) in &self.entries {
            if entry_path == &resolved {
                continue;
            }
            let (parent, name) = parent_and_name(&from_canonical(entry_path));
            if canonical(&parent) == resolved {
                let kind = match entry {
                    Entry::Dir => Kind::Dir,
                    Entry::File(_) => Kind::File,
                    Entry::Link(_) => Kind::Link,
                };
                result.push((name, kind));
            }
        }
        result.sort_by(|a, b| a.0.cmp(&b.0));
        Ok(result)
    }

    pub fn cwd(&self) -> String {
        self.cwd.clone()
    }

    pub fn chdir(&mut self, path: &str) -> Result<(), Error> {
        let resolved = self.resolve_path(path, false)?;
        match self.entries.get(&resolved) {
            Some(Entry::Dir) => {
                self.cwd = resolved;
                Ok(())
            }
            _ => Err(Error::NotDir(resolved)),
        }
    }

    pub fn rename(&mut self, from: &str, to: &str, overwrite: bool) -> Result<(), Error> {
        let base = from_canonical(&self.cwd);
        let source_parts = components(from, &base)?;
        let destination_parts = components(to, &base)?;

        if source_parts.is_empty() || destination_parts.is_empty() {
            return Err(Error::Root);
        }

        let (source_parent, source_name) = self.parent_for(&source_parts)?;
        let source = joined(&from_canonical(&source_parent), &source_name);
        let source_entry = self
            .entries
            .get(&source)
            .cloned()
            .ok_or_else(|| Error::Missing(source.clone()))?;

        if matches!(source_entry, Entry::Dir) && is_within(&self.cwd, &source) {
            return Err(Error::Busy(source));
        }

        let (destination_parent, destination_name) = self.parent_for(&destination_parts)?;
        let destination = joined(&from_canonical(&destination_parent), &destination_name);

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
            match (&source_entry, dest_entry) {
                (Entry::Dir, Entry::Dir) => {
                    if self.entries.keys().any(|p| p != &destination && is_within(p, &destination)) {
                        return Err(Error::NotEmpty(destination));
                    }
                }
                (Entry::Dir, _) => return Err(Error::NotDir(destination)),
                (_, Entry::Dir) => return Err(Error::IsDir(destination)),
                _ => {}
            }
        }

        let source_tree: Vec<(String, Entry)> = self
            .entries
            .iter()
            .filter(|(p, _)| is_within(p, &source))
            .map(|(p, e)| (p.clone(), e.clone()))
            .collect();

        if let Some(ref dest_entry) = destination_entry {
            if matches!(dest_entry, Entry::Dir) {
                self.entries.retain(|p, _| !is_within(p, &destination));
            } else {
                self.entries.remove(&destination);
            }
        }

        for (old_path, _) in &source_tree {
            self.entries.remove(old_path);
        }

        for (old_path, entry) in source_tree {
            let suffix = &old_path[source.len()..];
            let new_path = format!("{destination}{suffix}");
            self.entries.insert(new_path, entry);
        }

        Ok(())
    }

    pub fn remove(&mut self, path: &str, recursive: bool) -> Result<(), Error> {
        let parts = components(path, &from_canonical(&self.cwd))?;
        if parts.is_empty() {
            return Err(Error::Root);
        }

        let (parent, name) = self.parent_for(&parts)?;
        let resolved = joined(&from_canonical(&parent), &name);
        let entry = self
            .entries
            .get(&resolved)
            .cloned()
            .ok_or_else(|| Error::Missing(resolved.clone()))?;

        if matches!(entry, Entry::Dir) && is_within(&self.cwd, &resolved) {
            return Err(Error::Busy(resolved));
        }

        if matches!(entry, Entry::Dir) {
            let has_children = self
                .entries
                .keys()
                .any(|p| p != &resolved && is_within(p, &resolved));
            if has_children && !recursive {
                return Err(Error::NotEmpty(resolved));
            }
        }

        if matches!(entry, Entry::Dir) && recursive {
            self.entries.retain(|p, _| !is_within(p, &resolved));
        } else {
            self.entries.remove(&resolved);
        }
        Ok(())
    }

    pub fn symlink(&mut self, target: &str, link: &str) -> Result<(), Error> {
        let link_parts = components(link, &from_canonical(&self.cwd))?;
        if target.is_empty() {
            return Err(Error::InvalidTarget(0));
        }
        if let Some(i) = target.as_bytes().iter().position(|&b| b == 0) {
            return Err(Error::InvalidTarget(i));
        }

        if link_parts.is_empty() {
            return Err(Error::Exists("/".to_owned()));
        }
        let (parent, name) = self.parent_for(&link_parts)?;
        let resolved = joined(&from_canonical(&parent), &name);
        if self.entries.contains_key(&resolved) {
            return Err(Error::Exists(resolved));
        }
        self.entries.insert(resolved, Entry::Link(target.to_owned()));
        Ok(())
    }

    pub fn read_link(&self, path: &str) -> Result<String, Error> {
        let parts = components(path, &from_canonical(&self.cwd))?;
        if parts.is_empty() {
            return Err(Error::InvalidTarget(0));
        }
        let (parent_parts, name) = parent_and_name(&parts);
        let parent = self.resolve_parts(parent_parts, false)?;
        match self.entries.get(&parent) {
            None => Err(Error::Missing(parent)),
            Some(Entry::Dir) => {
                let final_path = joined(&from_canonical(&parent), &name);
                match self.entries.get(&final_path) {
                    None => Err(Error::Missing(final_path)),
                    Some(Entry::Link(target)) => Ok(target.clone()),
                    Some(_) => Err(Error::InvalidTarget(0)),
                }
            }
            Some(_) => Err(Error::NotDir(parent)),
        }
    }

    pub fn du(&self, path: &str) -> Result<Vec<(String, u64)>, Error> {
        let resolved = self.resolve_path(path, false)?;
        if !self.entries.contains_key(&resolved) {
            return Err(Error::Missing(resolved));
        }

        let mut rows: Vec<(String, u64)> = self
            .entries
            .iter()
            .filter(|(p, _)| is_within(p, &resolved))
            .map(|(p, entry)| {
                let size = match entry {
                    Entry::Dir => self
                        .entries
                        .iter()
                        .filter(|(child_path, child)| {
                            is_within(child_path, p)
                                && *child_path != p
                                && matches!(child, Entry::File(_) | Entry::Link(_))
                        })
                        .map(|(_, child)| match child {
                            Entry::File(data) => data.len() as u64,
                            Entry::Link(target) => target.len() as u64,
                            Entry::Dir => 0,
                        })
                        .sum(),
                    Entry::File(data) => data.len() as u64,
                    Entry::Link(target) => target.len() as u64,
                };
                (p.clone(), size)
            })
            .collect();

        rows.sort_by(|a, b| a.0.cmp(&b.0));
        Ok(rows)
    }
}

fn main() {}