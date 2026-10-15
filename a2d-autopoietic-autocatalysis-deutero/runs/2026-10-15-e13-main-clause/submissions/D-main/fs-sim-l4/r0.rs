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
        path.split('/')
            .filter(|part| !part.is_empty() && *part != ".")
            .fold(Vec::new(), |mut acc, part| {
                if part == ".." {
                    acc.pop();
                } else {
                    acc.push(part.to_string());
                }
                acc
            })
    }

    fn canonical(parts: &[String]) -> String {
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
        Self::canonical(&parts)
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

    fn basename(path: &str) -> &str {
        path.rsplit('/').next().unwrap_or("")
    }

    fn is_within(path: &str, dir: &str) -> bool {
        if dir == "/" {
            return path.starts_with('/');
        }
        path == dir
            || path
                .strip_prefix(dir)
                .is_some_and(|rest| rest.starts_with('/'))
    }

    fn join_parts(parts: &[String], suffix: &[String]) -> Vec<String> {
        let mut out = parts.to_vec();
        out.extend_from_slice(suffix);
        out
    }

    fn lookup(
        &self,
        path: &str,
        create_missing_final: bool,
    ) -> Result<String, Error> {
        let todo = Self::components(path);
        let mut resolved: Vec<String> = Vec::new();
        let mut index = 0;
        let mut visited = BTreeSet::new();
        let mut expansions = 0usize;

        while index < todo.len() {
            let name = todo[index].clone();
            let mut candidate_parts = resolved.clone();
            candidate_parts.push(name);
            let candidate = Self::canonical(&candidate_parts);

            match self.entries.get(&candidate) {
                None => {
                    if create_missing_final && index + 1 == todo.len() {
                        return Ok(candidate);
                    }
                    return Err(Error::Missing(candidate));
                }
                Some(Entry::Dir) => {
                    resolved.push(todo[index].clone());
                    index += 1;
                }
                Some(Entry::File(_)) => {
                    if index + 1 < todo.len() {
                        return Err(Error::NotDir(candidate));
                    }
                    return Ok(candidate);
                }
                Some(Entry::Link(target)) => {
                    if !visited.insert(candidate.clone()) {
                        return Err(Error::Loop(candidate));
                    }
                    if expansions >= 8 {
                        return Err(Error::Depth(candidate));
                    }
                    expansions += 1;

                    let mut target_parts = if target.starts_with('/') {
                        Vec::new()
                    } else {
                        resolved.clone()
                    };
                    for part in target.split('/') {
                        if part.is_empty() || part == "." {
                            continue;
                        }
                        if part == ".." {
                            target_parts.pop();
                        } else {
                            target_parts.push(part.to_string());
                        }
                    }
                    let suffix = &todo[index + 1..];
                    resolved = Self::join_parts(&target_parts, suffix);
                    let expanded = Self::canonical(&resolved);
                    let mut expanded_todo = Self::components(&expanded);
                    resolved.clear();
                    index = 0;
                    // Continue from root with the expanded path, retaining link state.
                    if expanded_todo.is_empty() {
                        return Ok("/".to_string());
                    }
                    // Replace the remaining path with its expanded canonical components.
                    // The outer loop state is represented by these locals.
                    let mut current_todo = expanded_todo;
                    loop {
                        if index >= current_todo.len() {
                            return Ok(Self::canonical(&resolved));
                        }
                        let next_name = current_todo[index].clone();
                        let mut next_parts = resolved.clone();
                        next_parts.push(next_name);
                        let next_path = Self::canonical(&next_parts);
                        match self.entries.get(&next_path) {
                            None => {
                                if create_missing_final && index + 1 == current_todo.len() {
                                    return Ok(next_path);
                                }
                                return Err(Error::Missing(next_path));
                            }
                            Some(Entry::Dir) => {
                                resolved.push(current_todo[index].clone());
                                index += 1;
                            }
                            Some(Entry::File(_)) => {
                                if index + 1 < current_todo.len() {
                                    return Err(Error::NotDir(next_path));
                                }
                                return Ok(next_path);
                            }
                            Some(Entry::Link(next_target)) => {
                                if !visited.insert(next_path.clone()) {
                                    return Err(Error::Loop(next_path));
                                }
                                if expansions >= 8 {
                                    return Err(Error::Depth(next_path));
                                }
                                expansions += 1;
                                let mut next_target_parts =
                                    if next_target.starts_with('/') {
                                        Vec::new()
                                    } else {
                                        resolved.clone()
                                    };
                                for part in next_target.split('/') {
                                    if part.is_empty() || part == "." {
                                        continue;
                                    }
                                    if part == ".." {
                                        next_target_parts.pop();
                                    } else {
                                        next_target_parts.push(part.to_string());
                                    }
                                }
                                let rest = current_todo[index + 1..].to_vec();
                                current_todo =
                                    Self::components(&Self::canonical(&Self::join_parts(
                                        &next_target_parts,
                                        &rest,
                                    )));
                                resolved.clear();
                                index = 0;
                            }
                        }
                    }
                }
            }
        }
        Ok(Self::canonical(&resolved))
    }

    fn resolve_parent(&self, path: &str) -> Result<(String, String), Error> {
        let parent_path = Self::parent(path);
        let basename = Self::basename(path).to_string();
        let resolved_parent = self.lookup(&parent_path, false)?;
        match self.entries.get(&resolved_parent) {
            Some(Entry::Dir) => Ok((resolved_parent, basename)),
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
            .filter(|p| *p != path && Self::is_within(p, path))
            .cloned()
            .collect()
    }

    fn dir_size(&self, path: &str) -> u64 {
        self.entries
            .iter()
            .filter(|(entry_path, _)| {
                **entry_path == path || Self::is_within(entry_path, path)
            })
            .map(|(_, entry)| match entry {
                Entry::File(bytes) => bytes.len() as u64,
                Entry::Link(target) => target.len() as u64,
                Entry::Dir => 0,
            })
            .sum()
    }

    pub fn mkdir(&mut self, path: &str) -> Result<(), Error> {
        Self::validate_path(path)?;
        let normalized = Self::normalize(path, &self.cwd);
        if normalized == "/" {
            return Err(Error::Exists("/".to_string()));
        }
        let (parent, name) = self.resolve_parent(&normalized)?;
        let final_path = Self::child_path(&parent, &name);
        if self.entries.contains_key(&final_path) {
            return Err(Error::Exists(final_path));
        }
        self.entries.insert(final_path, Entry::Dir);
        Ok(())
    }

    pub fn write(&mut self, path: &str, data: &[u8]) -> Result<(), Error> {
        Self::validate_path(path)?;
        let normalized = Self::normalize(path, &self.cwd);
        let resolved = self.lookup(&normalized, true)?;
        match self.entries.get(&resolved) {
            Some(Entry::Dir) => Err(Error::IsDir(resolved)),
            Some(Entry::File(_)) => {
                self.entries
                    .insert(resolved, Entry::File(data.to_vec()));
                Ok(())
            }
            Some(Entry::Link(_)) => unreachable!(),
            None => {
                let (parent, name) = self.resolve_parent(&resolved)?;
                let final_path = Self::child_path(&parent, &name);
                self.entries.insert(final_path, Entry::File(data.to_vec()));
                Ok(())
            }
        }
    }

    pub fn read(&self, path: &str) -> Result<Vec<u8>, Error> {
        Self::validate_path(path)?;
        let normalized = Self::normalize(path, &self.cwd);
        let resolved = self.lookup(&normalized, false)?;
        match self.entries.get(&resolved) {
            Some(Entry::File(data)) => Ok(data.clone()),
            Some(Entry::Dir) => Err(Error::IsDir(resolved)),
            Some(Entry::Link(_)) => unreachable!(),
            None => Err(Error::Missing(resolved)),
        }
    }

    pub fn ls(&self, path: &str) -> Result<Vec<(String, Kind)>, Error> {
        Self::validate_path(path)?;
        let normalized = Self::normalize(path, &self.cwd);
        let resolved = self.lookup(&normalized, false)?;
        match self.entries.get(&resolved) {
            Some(Entry::Dir) => {
                let mut rows: Vec<(String, Kind)> = self
                    .entries
                    .iter()
                    .filter_map(|(entry_path, entry)| {
                        if Self::parent(entry_path) == resolved {
                            Some((Self::basename(entry_path).to_string(), entry.kind()))
                        } else {
                            None
                        }
                    })
                    .collect();
                rows.sort_by(|a, b| a.0.cmp(&b.0));
                Ok(rows)
            }
            _ => Err(Error::NotDir(resolved)),
        }
    }

    pub fn cwd(&self) -> String {
        self.cwd.clone()
    }

    pub fn chdir(&mut self, path: &str) -> Result<(), Error> {
        Self::validate_path(path)?;
        let normalized = Self::normalize(path, &self.cwd);
        let resolved = self.lookup(&normalized, false)?;
        match self.entries.get(&resolved) {
            Some(Entry::Dir) => {
                self.cwd = resolved;
                Ok(())
            }
            _ => Err(Error::NotDir(resolved)),
        }
    }

    pub fn rename(
        &mut self,
        from: &str,
        to: &str,
        overwrite: bool,
    ) -> Result<(), Error> {
        Self::validate_path(from)?;
        Self::validate_path(to)?;
        let from_path = Self::normalize(from, &self.cwd);
        let to_path = Self::normalize(to, &self.cwd);

        if from_path == "/" || to_path == "/" {
            return Err(Error::Root);
        }

        let (source_parent, source_name) = self.resolve_parent(&from_path)?;
        let source = Self::child_path(&source_parent, &source_name);
        let source_entry = self
            .entries
            .get(&source)
            .cloned()
            .ok_or_else(|| Error::Missing(source.clone()))?;

        if matches!(source_entry, Entry::Dir) && Self::is_within(&self.cwd, &source) {
            return Err(Error::Busy(source));
        }

        let (dest_parent, dest_name) = self.resolve_parent(&to_path)?;
        let destination = Self::child_path(&dest_parent, &dest_name);

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
                    if !self.descendants(&destination).is_empty() {
                        return Err(Error::NotEmpty(destination));
                    }
                }
                (Entry::Dir, _) => return Err(Error::NotDir(destination)),
                (_, Entry::Dir) => return Err(Error::IsDir(destination)),
                _ => {}
            }
        }

        let mut moved: Vec<(String, Entry)> = self
            .entries
            .iter()
            .filter(|(p, _)| **p == source || Self::is_within(p, &source))
            .map(|(p, e)| (p.clone(), e.clone()))
            .collect();
        moved.sort_by_key(|(p, _)| p.len());

        if destination_entry.is_some() {
            let doomed = self
                .entries
                .keys()
                .filter(|p| **p == destination || Self::is_within(p, &destination))
                .cloned()
                .collect::<Vec<_>>();
            for p in doomed {
                self.entries.remove(&p);
            }
        }

        for (old, _) in &moved {
            self.entries.remove(old);
        }

        for (old, entry) in moved {
            let suffix = if old == source {
                String::new()
            } else {
                old[source.len()..].to_string()
            };
            self.entries.insert(format!("{}{}", destination, suffix), entry);
        }
        Ok(())
    }

    pub fn remove(&mut self, path: &str, recursive: bool) -> Result<(), Error> {
        Self::validate_path(path)?;
        let normalized = Self::normalize(path, &self.cwd);
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

        if matches!(entry, Entry::Dir) && Self::is_within(&self.cwd, &target) {
            return Err(Error::Busy(target));
        }

        let descendants = self.descendants(&target);
        if matches!(entry, Entry::Dir) && !recursive && !descendants.is_empty() {
            return Err(Error::NotEmpty(target));
        }

        for p in descendants {
            self.entries.remove(&p);
        }
        self.entries.remove(&target);
        Ok(())
    }

    pub fn symlink(&mut self, target: &str, link: &str) -> Result<(), Error> {
        Self::validate_path(link)?;
        if target.is_empty() {
            return Err(Error::InvalidTarget(0));
        }
        if let Some(i) = target.as_bytes().iter().position(|&b| b == 0) {
            return Err(Error::InvalidTarget(i));
        }

        let normalized = Self::normalize(link, &self.cwd);
        if normalized == "/" {
            return Err(Error::Exists("/".to_string()));
        }
        let (parent, name) = self.resolve_parent(&normalized)?;
        let final_path = Self::child_path(&parent, &name);
        if self.entries.contains_key(&final_path) {
            return Err(Error::Exists(final_path));
        }
        self.entries
            .insert(final_path, Entry::Link(target.to_string()));
        Ok(())
    }

    pub fn read_link(&self, path: &str) -> Result<String, Error> {
        Self::validate_path(path)?;
        let normalized = Self::normalize(path, &self.cwd);
        if normalized == "/" {
            return Err(Error::InvalidTarget(0));
        }
        let (parent, name) = self.resolve_parent(&normalized)?;
        let final_path = Self::child_path(&parent, &name);
        match self.entries.get(&final_path) {
            Some(Entry::Link(target)) => Ok(target.clone()),
            Some(_) => Err(Error::InvalidTarget(0)),
            None => Err(Error::Missing(final_path)),
        }
    }

    pub fn du(&self, path: &str) -> Result<Vec<(String, u64)>, Error> {
        Self::validate_path(path)?;
        let normalized = Self::normalize(path, &self.cwd);
        let resolved = self.lookup(&normalized, false)?;
        if !self.entries.contains_key(&resolved) {
            return Err(Error::Missing(resolved));
        }

        let mut paths = vec![resolved.clone()];
        paths.extend(self.descendants(&resolved));
        paths.sort();

        Ok(paths
            .into_iter()
            .map(|entry_path| {
                let size = self
                    .entries
                    .get(&entry_path)
                    .map(|entry| match entry {
                        Entry::File(bytes) => bytes.len() as u64,
                        Entry::Link(target) => target.len() as u64,
                        Entry::Dir => self.dir_size(&entry_path),
                    })
                    .unwrap_or(0);
                (entry_path, size)
            })
            .collect())
    }
}

fn main() {}