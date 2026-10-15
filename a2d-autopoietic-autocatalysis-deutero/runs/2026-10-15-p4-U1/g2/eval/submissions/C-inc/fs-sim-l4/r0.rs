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

use std::collections::{BTreeMap, HashSet};

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
            "/".to_string()
        } else {
            format!("/{}", parts.join("/"))
        }
    }

    fn normalize(&self, path: &str) -> Result<String, Error> {
        Self::validate(path)?;
        Ok(Self::normalize_from(path, &self.cwd))
    }

    fn parent(path: &str) -> (String, String) {
        if path == "/" {
            return ("/".to_string(), String::new());
        }
        match path.rfind('/') {
            Some(0) => ("/".to_string(), path[1..].to_string()),
            Some(i) => (path[..i].to_string(), path[i + 1..].to_string()),
            None => ("/".to_string(), path.to_string()),
        }
    }

    fn join(parent: &str, name: &str) -> String {
        if parent == "/" {
            format!("/{}", name)
        } else {
            format!("{}/{}", parent, name)
        }
    }

    fn is_within(path: &str, dir: &str) -> bool {
        path == dir
            || if dir == "/" {
                path.starts_with('/')
            } else {
                path.strip_prefix(dir)
                    .map(|suffix| suffix.starts_with('/'))
                    .unwrap_or(false)
            }
    }

    fn resolve(
        &self,
        initial: &str,
        follow_final: bool,
        allow_missing_final: bool,
    ) -> Result<String, Error> {
        let mut current = String::new();
        let mut pending: Vec<String> = initial
            .split('/')
            .filter(|s| !s.is_empty())
            .map(str::to_string)
            .collect();
        let mut visited = HashSet::new();
        let mut expansions = 0usize;

        while !pending.is_empty() {
            let component = pending.remove(0);
            let candidate = Self::join(&current, &component);
            let final_component = pending.is_empty();

            match self.entries.get(&candidate) {
                None => {
                    if final_component && allow_missing_final {
                        return Ok(candidate);
                    }
                    return Err(Error::Missing(candidate));
                }
                Some(Entry::Dir) => current = candidate,
                Some(Entry::File(_)) => {
                    if !final_component {
                        return Err(Error::NotDir(candidate));
                    }
                    current = candidate;
                }
                Some(Entry::Link(target)) if !final_component || follow_final => {
                    if !visited.insert(candidate.clone()) {
                        return Err(Error::Loop(candidate));
                    }
                    if expansions >= 8 {
                        return Err(Error::Depth(candidate));
                    }
                    expansions += 1;

                    let physical_parent = Self::parent(&candidate).0;
                    let mut expanded = Self::normalize_from(target, &physical_parent);
                    if !pending.is_empty() {
                        let suffix = pending.join("/");
                        expanded =
                            Self::normalize_from(&format!("{}/{}", expanded, suffix), "/");
                    }
                    current.clear();
                    pending = expanded
                        .split('/')
                        .filter(|s| !s.is_empty())
                        .map(str::to_string)
                        .collect();
                }
                Some(Entry::Link(_)) => current = candidate,
            }
        }

        if current.is_empty() {
            Ok("/".to_string())
        } else {
            Ok(current)
        }
    }

    fn resolve_parent(&self, path: &str) -> Result<(String, String, String), Error> {
        let (parent, name) = Self::parent(path);
        let resolved_parent = self.resolve(&parent, true, false)?;
        match self.entries.get(&resolved_parent) {
            Some(Entry::Dir) => {
                let final_path = Self::join(&resolved_parent, &name);
                Ok((resolved_parent, name, final_path))
            }
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

    fn has_children(&self, path: &str) -> bool {
        self.entries
            .keys()
            .any(|candidate| candidate != path && Self::is_within(candidate, path))
    }

    fn remove_subtree(&mut self, path: &str) {
        self.entries
            .retain(|candidate, _| !Self::is_within(candidate, path));
    }

    pub fn mkdir(&mut self, path: &str) -> Result<(), Error> {
        let canonical = self.normalize(path)?;
        let (_, _, final_path) = self.resolve_parent(&canonical)?;
        if self.entries.contains_key(&final_path) {
            return Err(Error::Exists(final_path));
        }
        self.entries.insert(final_path, Entry::Dir);
        Ok(())
    }

    pub fn write(&mut self, path: &str, data: &[u8]) -> Result<(), Error> {
        let canonical = self.normalize(path)?;
        let resolved = self.resolve(&canonical, true, true)?;
        if matches!(self.entries.get(&resolved), Some(Entry::Dir)) {
            return Err(Error::IsDir(resolved));
        }
        if !self.entries.contains_key(&resolved) {
            let (parent, _) = Self::parent(&resolved);
            match self.entries.get(&parent) {
                Some(Entry::Dir) => {}
                Some(_) => return Err(Error::NotDir(parent)),
                None => return Err(Error::Missing(parent)),
            }
        }
        self.entries.insert(resolved, Entry::File(data.to_vec()));
        Ok(())
    }

    pub fn read(&self, path: &str) -> Result<Vec<u8>, Error> {
        let canonical = self.normalize(path)?;
        let resolved = self.resolve(&canonical, true, false)?;
        match self.entries.get(&resolved) {
            Some(Entry::File(data)) => Ok(data.clone()),
            Some(Entry::Dir) => Err(Error::IsDir(resolved)),
            Some(Entry::Link(_)) => Err(Error::Loop(resolved)),
            None => Err(Error::Missing(resolved)),
        }
    }

    pub fn ls(&self, path: &str) -> Result<Vec<(String, Kind)>, Error> {
        let canonical = self.normalize(path)?;
        let resolved = self.resolve(&canonical, true, false)?;
        match self.entries.get(&resolved) {
            Some(Entry::Dir) => {}
            Some(_) => return Err(Error::NotDir(resolved)),
            None => return Err(Error::Missing(resolved)),
        }

        let mut children = Vec::new();
        for (candidate, entry) in &self.entries {
            if candidate == &resolved {
                continue;
            }
            let (parent, name) = Self::parent(candidate);
            if parent == resolved {
                children.push((name, Self::kind(entry)));
            }
        }
        children.sort_by(|a, b| a.0.cmp(&b.0));
        Ok(children)
    }

    pub fn cwd(&self) -> String {
        self.cwd.clone()
    }

    pub fn chdir(&mut self, path: &str) -> Result<(), Error> {
        let canonical = self.normalize(path)?;
        let resolved = self.resolve(&canonical, true, false)?;
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

        let (_, _, source_path) = self.resolve_parent(&source)?;
        let source_entry = self
            .entries
            .get(&source_path)
            .cloned()
            .ok_or_else(|| Error::Missing(source_path.clone()))?;

        if matches!(source_entry, Entry::Dir) && Self::is_within(&self.cwd, &source_path) {
            return Err(Error::Busy(source_path));
        }

        let (_, _, destination_path) = self.resolve_parent(&destination)?;
        if matches!(self.entries.get(&destination_path), Some(Entry::Dir))
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

        let moved: Vec<(String, Entry)> = self
            .entries
            .iter()
            .filter(|(candidate, _)| Self::is_within(candidate, &source_path))
            .map(|(candidate, entry)| (candidate.clone(), entry.clone()))
            .collect();

        if destination_entry.is_some() {
            self.remove_subtree(&destination_path);
        }
        self.remove_subtree(&source_path);

        for (old_path, entry) in moved {
            let suffix = &old_path[source_path.len()..];
            let new_path = format!("{}{}", destination_path, suffix);
            self.entries.insert(new_path, entry);
        }
        Ok(())
    }

    pub fn remove(&mut self, path: &str, recursive: bool) -> Result<(), Error> {
        let canonical = self.normalize(path)?;
        if canonical == "/" {
            return Err(Error::Root);
        }

        let (_, _, resolved) = self.resolve_parent(&canonical)?;
        let entry = self
            .entries
            .get(&resolved)
            .cloned()
            .ok_or_else(|| Error::Missing(resolved.clone()))?;

        if matches!(entry, Entry::Dir) && Self::is_within(&self.cwd, &resolved) {
            return Err(Error::Busy(resolved));
        }
        if matches!(entry, Entry::Dir) && self.has_children(&resolved) && !recursive {
            return Err(Error::NotEmpty(resolved));
        }

        self.remove_subtree(&resolved);
        Ok(())
    }

    pub fn symlink(&mut self, target: &str, link: &str) -> Result<(), Error> {
        let canonical_link = self.normalize(link)?;
        if target.is_empty() {
            return Err(Error::InvalidTarget(0));
        }
        if let Some(i) = target.as_bytes().iter().position(|&b| b == 0) {
            return Err(Error::InvalidTarget(i));
        }

        let (_, _, resolved_link) = self.resolve_parent(&canonical_link)?;
        if self.entries.contains_key(&resolved_link) {
            return Err(Error::Exists(resolved_link));
        }
        self.entries
            .insert(resolved_link, Entry::Link(target.to_string()));
        Ok(())
    }

    pub fn read_link(&self, path: &str) -> Result<String, Error> {
        let canonical = self.normalize(path)?;
        let resolved = self.resolve(&canonical, false, false)?;
        match self.entries.get(&resolved) {
            Some(Entry::Link(target)) => Ok(target.clone()),
            Some(_) => Err(Error::InvalidTarget(0)),
            None => Err(Error::Missing(resolved)),
        }
    }

    pub fn du(&self, path: &str) -> Result<Vec<(String, u64)>, Error> {
        let canonical = self.normalize(path)?;
        let resolved = self.resolve(&canonical, true, false)?;
        let root_entry = self
            .entries
            .get(&resolved)
            .ok_or_else(|| Error::Missing(resolved.clone()))?;

        let mut rows: Vec<(String, u64)> = self
            .entries
            .iter()
            .filter(|(candidate, _)| {
                *candidate == &resolved || Self::is_within(candidate, &resolved)
            })
            .map(|(candidate, entry)| {
                let size = match entry {
                    Entry::File(data) => data.len() as u64,
                    Entry::Link(target) => target.len() as u64,
                    Entry::Dir => 0,
                };
                (candidate.clone(), size)
            })
            .collect();

        let total: u64 = rows.iter().map(|(_, size)| *size).sum();
        if matches!(root_entry, Entry::Dir) {
            if let Some(row) = rows.iter_mut().find(|(candidate, _)| candidate == &resolved) {
                row.1 = total;
            }
        }
        rows.sort_by(|a, b| a.0.cmp(&b.0));
        Ok(rows)
    }
}

fn main() {}