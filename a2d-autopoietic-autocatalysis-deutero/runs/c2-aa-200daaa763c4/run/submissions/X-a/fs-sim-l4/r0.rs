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

    pub fn mkdir(&mut self, path: &str) -> Result<(), Error> {
        let canonical = normalize(path, &self.cwd)?;
        let (parent, name) = split_parent(&canonical);
        let resolved_parent = self.resolve(&parent, true, false)?;
        if !matches!(self.entries.get(&resolved_parent), Some(Entry::Dir)) {
            return Err(Error::NotDir(resolved_parent));
        }
        let destination = join_child(&resolved_parent, &name);
        if self.entries.contains_key(&destination) {
            return Err(Error::Exists(destination));
        }
        self.entries.insert(destination, Entry::Dir);
        Ok(())
    }

    pub fn write(&mut self, path: &str, data: &[u8]) -> Result<(), Error> {
        let canonical = normalize(path, &self.cwd)?;
        let resolved = self.resolve(&canonical, true, true)?;
        match self.entries.get(&resolved) {
            Some(Entry::Dir) => Err(Error::IsDir(resolved)),
            Some(Entry::File(_)) | None => {
                self.entries.insert(resolved, Entry::File(data.to_vec()));
                Ok(())
            }
            Some(Entry::Link(_)) => unreachable!(),
        }
    }

    pub fn read(&self, path: &str) -> Result<Vec<u8>, Error> {
        let canonical = normalize(path, &self.cwd)?;
        let resolved = self.resolve(&canonical, true, false)?;
        match self.entries.get(&resolved) {
            Some(Entry::File(data)) => Ok(data.clone()),
            Some(Entry::Dir) => Err(Error::IsDir(resolved)),
            _ => Err(Error::Missing(resolved)),
        }
    }

    pub fn ls(&self, path: &str) -> Result<Vec<(String, Kind)>, Error> {
        let canonical = normalize(path, &self.cwd)?;
        let resolved = self.resolve(&canonical, true, false)?;
        if !matches!(self.entries.get(&resolved), Some(Entry::Dir)) {
            return Err(Error::NotDir(resolved));
        }
        let mut result = Vec::new();
        for (entry_path, entry) in &self.entries {
            if entry_path == &resolved {
                continue;
            }
            let (parent, name) = split_parent(entry_path);
            if parent == resolved {
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
        let canonical = normalize(path, &self.cwd)?;
        let resolved = self.resolve(&canonical, true, false)?;
        if !matches!(self.entries.get(&resolved), Some(Entry::Dir)) {
            return Err(Error::NotDir(resolved));
        }
        self.cwd = resolved;
        Ok(())
    }

    pub fn rename(&mut self, from: &str, to: &str, overwrite: bool) -> Result<(), Error> {
        let from = normalize(from, &self.cwd)?;
        let to = normalize(to, &self.cwd)?;
        if from == "/" || to == "/" {
            return Err(Error::Root);
        }

        let (from_parent_raw, from_name) = split_parent(&from);
        let (to_parent_raw, to_name) = split_parent(&to);

        let from_parent = self.resolve(&from_parent_raw, true, false)?;
        if !matches!(self.entries.get(&from_parent), Some(Entry::Dir)) {
            return Err(Error::NotDir(from_parent));
        }
        let source = join_child(&from_parent, &from_name);
        let source_entry = self
            .entries
            .get(&source)
            .cloned()
            .ok_or_else(|| Error::Missing(source.clone()))?;

        if matches!(source_entry, Entry::Dir) && is_within(&self.cwd, &source) {
            return Err(Error::Busy(source));
        }

        let to_parent = self.resolve(&to_parent_raw, true, false)?;
        if !matches!(self.entries.get(&to_parent), Some(Entry::Dir)) {
            return Err(Error::NotDir(to_parent));
        }
        let destination = join_child(&to_parent, &to_name);

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

        if let Some(dest_entry) = &destination_entry {
            match (&source_entry, dest_entry) {
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

        let moving: Vec<(String, Entry)> = self
            .entries
            .iter()
            .filter(|(path, _)| is_within(path, &source))
            .map(|(path, entry)| (path.clone(), entry.clone()))
            .collect();

        if destination_entry.is_some() {
            self.remove_subtree(&destination);
        }
        self.remove_subtree(&source);

        for (old_path, entry) in moving {
            let suffix = old_path.strip_prefix(&source).unwrap_or("");
            let new_path = if suffix.is_empty() {
                destination.clone()
            } else if destination == "/" {
                format!("/{suffix}")
            } else {
                format!("{destination}{suffix}")
            };
            self.entries.insert(new_path, entry);
        }
        Ok(())
    }

    pub fn remove(&mut self, path: &str, recursive: bool) -> Result<(), Error> {
        let canonical = normalize(path, &self.cwd)?;
        if canonical == "/" {
            return Err(Error::Root);
        }
        let (parent_raw, name) = split_parent(&canonical);
        let parent = self.resolve(&parent_raw, true, false)?;
        if !matches!(self.entries.get(&parent), Some(Entry::Dir)) {
            return Err(Error::NotDir(parent));
        }
        let target = join_child(&parent, &name);
        let entry = self
            .entries
            .get(&target)
            .ok_or_else(|| Error::Missing(target.clone()))?;
        if matches!(entry, Entry::Dir) && is_within(&self.cwd, &target) {
            return Err(Error::Busy(target));
        }
        if matches!(entry, Entry::Dir) && self.has_children(&target) && !recursive {
            return Err(Error::NotEmpty(target));
        }
        self.remove_subtree(&target);
        Ok(())
    }

    pub fn symlink(&mut self, target: &str, link: &str) -> Result<(), Error> {
        let link = normalize(link, &self.cwd)?;
        validate_target(target)?;
        let (parent_raw, name) = split_parent(&link);
        let parent = self.resolve(&parent_raw, true, false)?;
        if !matches!(self.entries.get(&parent), Some(Entry::Dir)) {
            return Err(Error::NotDir(parent));
        }
        let destination = join_child(&parent, &name);
        if self.entries.contains_key(&destination) {
            return Err(Error::Exists(destination));
        }
        self.entries
            .insert(destination, Entry::Link(target.to_string()));
        Ok(())
    }

    pub fn read_link(&self, path: &str) -> Result<String, Error> {
        let canonical = normalize(path, &self.cwd)?;
        let (parent_raw, name) = split_parent(&canonical);
        let parent = self.resolve(&parent_raw, true, false)?;
        if !matches!(self.entries.get(&parent), Some(Entry::Dir)) {
            return Err(Error::NotDir(parent));
        }
        let final_path = join_child(&parent, &name);
        match self.entries.get(&final_path) {
            Some(Entry::Link(target)) => Ok(target.clone()),
            Some(_) => Err(Error::InvalidTarget(0)),
            None => Err(Error::Missing(final_path)),
        }
    }

    pub fn du(&self, path: &str) -> Result<Vec<(String, u64)>, Error> {
        let canonical = normalize(path, &self.cwd)?;
        let resolved = self.resolve(&canonical, true, false)?;
        let root_entry = self
            .entries
            .get(&resolved)
            .ok_or_else(|| Error::Missing(resolved.clone()))?;
        if !matches!(root_entry, Entry::Dir | Entry::File(_)) {
            return Err(Error::Missing(resolved));
        }

        let mut rows = Vec::new();
        for (entry_path, entry) in &self.entries {
            if is_within(entry_path, &resolved) {
                let size = match entry {
                    Entry::Dir => self.subtree_data_size(entry_path),
                    Entry::File(data) => data.len() as u64,
                    Entry::Link(target) => target.len() as u64,
                };
                rows.push((entry_path.clone(), size));
            }
        }
        rows.sort_by(|a, b| a.0.cmp(&b.0));
        Ok(rows)
    }

    fn resolve(&self, path: &str, follow_final: bool, create_missing_final: bool) -> Result<String, Error> {
        let mut current = path.to_string();
        let mut visited = BTreeSet::new();
        let mut expansions = 0usize;

        loop {
            let components = components(&current);
            let mut physical = String::from("/");
            let mut restart: Option<String> = None;

            for (index, component) in components.iter().enumerate() {
                let candidate = join_child(&physical, component);
                let is_final = index + 1 == components.len();
                let should_follow = !is_final || follow_final;

                if !self.entries.contains_key(&candidate) {
                    if is_final && create_missing_final {
                        return Ok(candidate);
                    }
                    return Err(Error::Missing(candidate));
                }

                if should_follow {
                    if let Some(Entry::Link(target)) = self.entries.get(&candidate) {
                        if !visited.insert(candidate.clone()) {
                            return Err(Error::Loop(candidate));
                        }
                        if expansions >= 8 {
                            return Err(Error::Depth(candidate));
                        }
                        expansions += 1;

                        let target_base = if target.starts_with('/') {
                            normalize_unvalidated(target, "/")
                        } else {
                            let (link_parent, _) = split_parent(&candidate);
                            normalize_unvalidated(target, &link_parent)
                        };
                        let suffix = components[index + 1..].join("/");
                        restart = Some(if suffix.is_empty() {
                            target_base
                        } else if target_base == "/" {
                            format!("/{suffix}")
                        } else {
                            format!("{target_base}/{suffix}")
                        });
                        break;
                    }
                }

                let entry = self.entries.get(&candidate).expect("checked above");
                if !is_final && !matches!(entry, Entry::Dir) {
                    return Err(Error::NotDir(candidate));
                }
                physical = candidate;
            }

            if let Some(next) = restart {
                current = normalize_unvalidated(&next, "/");
                continue;
            }
            return Ok(physical);
        }
    }

    fn has_children(&self, path: &str) -> bool {
        self.entries
            .keys()
            .any(|entry_path| entry_path != path && is_within(entry_path, path))
    }

    fn remove_subtree(&mut self, path: &str) {
        let doomed: Vec<String> = self
            .entries
            .keys()
            .filter(|entry_path| is_within(entry_path, path))
            .cloned()
            .collect();
        for entry_path in doomed {
            self.entries.remove(&entry_path);
        }
    }

    fn subtree_data_size(&self, path: &str) -> u64 {
        self.entries
            .iter()
            .filter(|(entry_path, _)| *entry_path != path && is_within(entry_path, path))
            .map(|(_, entry)| match entry {
                Entry::Dir => 0,
                Entry::File(data) => data.len() as u64,
                Entry::Link(target) => target.len() as u64,
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

fn normalize(path: &str, cwd: &str) -> Result<String, Error> {
    validate_path(path)?;
    Ok(normalize_unvalidated(path, cwd))
}

fn normalize_unvalidated(path: &str, cwd: &str) -> String {
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
            other => parts.push(other.to_string()),
        }
    }
    if parts.is_empty() {
        "/".to_string()
    } else {
        format!("/{}", parts.join("/"))
    }
}

fn components(path: &str) -> Vec<String> {
    if path == "/" {
        Vec::new()
    } else {
        path.split('/')
            .filter(|part| !part.is_empty())
            .map(str::to_string)
            .collect()
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

fn join_child(parent: &str, name: &str) -> String {
    if parent == "/" {
        format!("/{name}")
    } else {
        format!("{parent}/{name}")
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

fn validate_target(target: &str) -> Result<(), Error> {
    if target.is_empty() {
        return Err(Error::InvalidTarget(0));
    }
    if let Some(index) = target.as_bytes().iter().position(|byte| *byte == 0) {
        return Err(Error::InvalidTarget(index));
    }
    Ok(())
}