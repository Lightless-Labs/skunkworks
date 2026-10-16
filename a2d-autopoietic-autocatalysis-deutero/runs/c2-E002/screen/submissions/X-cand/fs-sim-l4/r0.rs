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

    pub fn mkdir(&mut self, path: &str) -> Result<(), Error> {
        let canonical = self.normalize(path)?;
        if canonical == "/" {
            return Err(Error::Exists(canonical));
        }
        let (parent, name) = split_path(&canonical);
        let resolved_parent = self.resolve_full(&parent)?;
        self.require_dir(&resolved_parent)?;
        let destination = join_path(&resolved_parent, &name);
        if self.entries.contains_key(&destination) {
            return Err(Error::Exists(destination));
        }
        self.entries.insert(destination, Entry::Dir);
        Ok(())
    }

    pub fn write(&mut self, path: &str, data: &[u8]) -> Result<(), Error> {
        let canonical = self.normalize(path)?;
        let resolved = self.resolve_write(&canonical)?;
        if resolved == "/" {
            return Err(Error::IsDir(resolved));
        }
        match self.entries.get(&resolved) {
            Some(Entry::Dir) => Err(Error::IsDir(resolved)),
            Some(Entry::File(_)) => {
                self.entries.insert(resolved, Entry::File(data.to_vec()));
                Ok(())
            }
            Some(Entry::Link(_)) => unreachable!(),
            None => {
                let (parent, _) = split_path(&resolved);
                self.require_dir(&parent)?;
                self.entries.insert(resolved, Entry::File(data.to_vec()));
                Ok(())
            }
        }
    }

    pub fn read(&self, path: &str) -> Result<Vec<u8>, Error> {
        let canonical = self.normalize(path)?;
        let resolved = self.resolve_full(&canonical)?;
        match self.entries.get(&resolved) {
            Some(Entry::File(data)) => Ok(data.clone()),
            Some(Entry::Dir) => Err(Error::IsDir(resolved)),
            Some(Entry::Link(_)) | None => Err(Error::Missing(resolved)),
        }
    }

    pub fn ls(&self, path: &str) -> Result<Vec<(String, Kind)>, Error> {
        let canonical = self.normalize(path)?;
        let resolved = self.resolve_full(&canonical)?;
        match self.entries.get(&resolved) {
            Some(Entry::Dir) => {}
            _ => return Err(Error::NotDir(resolved)),
        }

        let mut children = Vec::new();
        for (entry_path, entry) in &self.entries {
            if entry_path == "/" || entry_path == &resolved {
                continue;
            }
            let (parent, name) = split_path(entry_path);
            if parent == resolved {
                let kind = match entry {
                    Entry::Dir => Kind::Dir,
                    Entry::File(_) => Kind::File,
                    Entry::Link(_) => Kind::Link,
                };
                children.push((name, kind));
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
        let resolved = self.resolve_full(&canonical)?;
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

        let (source_parent, source_name) = split_path(&source);
        let resolved_source_parent = self.resolve_full(&source_parent)?;
        self.require_dir(&resolved_source_parent)?;
        let resolved_source = join_path(&resolved_source_parent, &source_name);
        let source_entry = self
            .entries
            .get(&resolved_source)
            .cloned()
            .ok_or_else(|| Error::Missing(resolved_source.clone()))?;

        if matches!(source_entry, Entry::Dir) && is_within(&self.cwd, &resolved_source) {
            return Err(Error::Busy(resolved_source));
        }

        let (destination_parent, destination_name) = split_path(&destination);
        let resolved_destination_parent = self.resolve_full(&destination_parent)?;
        self.require_dir(&resolved_destination_parent)?;
        let resolved_destination = join_path(&resolved_destination_parent, &destination_name);

        if matches!(self.entries.get(&resolved_destination), Some(Entry::Dir))
            && is_within(&self.cwd, &resolved_destination)
        {
            return Err(Error::Busy(resolved_destination));
        }

        if resolved_source == resolved_destination {
            return Ok(());
        }

        if matches!(source_entry, Entry::Dir)
            && is_proper_descendant(&resolved_destination, &resolved_source)
        {
            return Err(Error::Cycle(resolved_destination));
        }

        let destination_entry = self.entries.get(&resolved_destination).cloned();
        if destination_entry.is_some() && !overwrite {
            return Err(Error::Exists(resolved_destination));
        }

        if let Some(ref target_entry) = destination_entry {
            match (&source_entry, target_entry) {
                (Entry::Dir, Entry::Dir) => {
                    if self.has_children(&resolved_destination) {
                        return Err(Error::NotEmpty(resolved_destination));
                    }
                }
                (Entry::Dir, _) => return Err(Error::NotDir(resolved_destination)),
                (_, Entry::Dir) => return Err(Error::IsDir(resolved_destination)),
                _ => {}
            }
        }

        let moved: Vec<(String, Entry)> = self
            .entries
            .iter()
            .filter(|(path, _)| is_within(path, &resolved_source))
            .map(|(path, entry)| (path.clone(), entry.clone()))
            .collect();

        if destination_entry.is_some() {
            self.entries
                .retain(|path, _| !is_within(path, &resolved_destination));
        }
        self.entries
            .retain(|path, _| !is_within(path, &resolved_source));

        for (old_path, entry) in moved {
            let suffix = old_path
                .strip_prefix(&resolved_source)
                .expect("source subtree prefix");
            let new_path = format!("{resolved_destination}{suffix}");
            self.entries.insert(new_path, entry);
        }
        Ok(())
    }

    pub fn remove(&mut self, path: &str, recursive: bool) -> Result<(), Error> {
        let canonical = self.normalize(path)?;
        if canonical == "/" {
            return Err(Error::Root);
        }
        let (parent, name) = split_path(&canonical);
        let resolved_parent = self.resolve_full(&parent)?;
        self.require_dir(&resolved_parent)?;
        let resolved = join_path(&resolved_parent, &name);
        let entry = self
            .entries
            .get(&resolved)
            .cloned()
            .ok_or_else(|| Error::Missing(resolved.clone()))?;

        if matches!(entry, Entry::Dir) && is_within(&self.cwd, &resolved) {
            return Err(Error::Busy(resolved));
        }
        if matches!(entry, Entry::Dir) && self.has_children(&resolved) && !recursive {
            return Err(Error::NotEmpty(resolved));
        }

        self.entries.retain(|candidate, _| !is_within(candidate, &resolved));
        Ok(())
    }

    pub fn symlink(&mut self, target: &str, link: &str) -> Result<(), Error> {
        let canonical_link = self.normalize(link)?;
        validate_target(target)?;

        if canonical_link == "/" {
            return Err(Error::Exists(canonical_link));
        }
        let (parent, name) = split_path(&canonical_link);
        let resolved_parent = self.resolve_full(&parent)?;
        self.require_dir(&resolved_parent)?;
        let resolved_link = join_path(&resolved_parent, &name);
        if self.entries.contains_key(&resolved_link) {
            return Err(Error::Exists(resolved_link));
        }
        self.entries
            .insert(resolved_link, Entry::Link(target.to_string()));
        Ok(())
    }

    pub fn read_link(&self, path: &str) -> Result<String, Error> {
        let canonical = self.normalize(path)?;
        if canonical == "/" {
            return Err(Error::InvalidTarget(0));
        }
        let (parent, name) = split_path(&canonical);
        let resolved_parent = self.resolve_full(&parent)?;
        self.require_dir(&resolved_parent)?;
        let resolved = join_path(&resolved_parent, &name);
        match self.entries.get(&resolved) {
            Some(Entry::Link(target)) => Ok(target.clone()),
            Some(_) => Err(Error::InvalidTarget(0)),
            None => Err(Error::Missing(resolved)),
        }
    }

    pub fn du(&self, path: &str) -> Result<Vec<(String, u64)>, Error> {
        let canonical = self.normalize(path)?;
        let resolved = self.resolve_full(&canonical)?;
        let entry = self
            .entries
            .get(&resolved)
            .ok_or_else(|| Error::Missing(resolved.clone()))?;
        if !matches!(entry, Entry::Dir | Entry::File(_) | Entry::Link(_)) {
            return Err(Error::Missing(resolved));
        }

        let mut rows = Vec::new();
        let mut descendants: Vec<(&String, &Entry)> = self
            .entries
            .iter()
            .filter(|(entry_path, _)| is_within(entry_path, &resolved))
            .collect();
        descendants.sort_by(|a, b| a.0.cmp(b.0));

        for (entry_path, entry) in descendants {
            let size = match entry {
                Entry::File(bytes) => bytes.len() as u64,
                Entry::Link(target) => target.len() as u64,
                Entry::Dir => self
                    .entries
                    .iter()
                    .filter(|(candidate, child)| {
                        matches!(child, Entry::File(_) | Entry::Link(_))
                            && is_within(candidate, entry_path)
                    })
                    .map(|(_, child)| match child {
                        Entry::File(bytes) => bytes.len() as u64,
                        Entry::Link(target) => target.len() as u64,
                        Entry::Dir => 0,
                    })
                    .sum(),
            };
            rows.push((entry_path.clone(), size));
        }
        Ok(rows)
    }

    fn normalize(&self, path: &str) -> Result<String, Error> {
        if let Some(index) = path.as_bytes().iter().position(|byte| *byte == 0) {
            return Err(Error::InvalidPath(index));
        }
        if path.is_empty() {
            return Err(Error::InvalidPath(0));
        }

        let absolute = path.starts_with('/');
        let mut components: Vec<String> = if absolute {
            Vec::new()
        } else {
            components(&self.cwd)
        };

        for component in path.split('/') {
            match component {
                "" | "." => {}
                ".." => {
                    components.pop();
                }
                other => components.push(other.to_string()),
            }
        }
        Ok(make_path(&components))
    }

    fn require_dir(&self, path: &str) -> Result<(), Error> {
        match self.entries.get(path) {
            Some(Entry::Dir) => Ok(()),
            Some(_) => Err(Error::NotDir(path.to_string())),
            None => Err(Error::Missing(path.to_string())),
        }
    }

    fn resolve_full(&self, path: &str) -> Result<String, Error> {
        self.resolve(path, true)
    }

    fn resolve(&self, path: &str, follow_final: bool) -> Result<String, Error> {
        let mut current = path.to_string();
        let mut visited = HashSet::new();
        let mut expansions = 0usize;

        loop {
            let parts = components(&current);
            let mut prefix_parts: Vec<String> = Vec::new();
            let mut restarted = false;

            for (index, component) in parts.iter().enumerate() {
                prefix_parts.push(component.clone());
                let prefix = make_path(&prefix_parts);
                let is_final = index + 1 == parts.len();

                match self.entries.get(&prefix) {
                    None => return Err(Error::Missing(prefix)),
                    Some(Entry::Dir) => {}
                    Some(Entry::File(_)) => {
                        if !is_final {
                            return Err(Error::NotDir(prefix));
                        }
                    }
                    Some(Entry::Link(target)) if !is_final || follow_final => {
                        if !visited.insert(prefix.clone()) {
                            return Err(Error::Loop(prefix));
                        }
                        if expansions >= 8 {
                            return Err(Error::Depth(prefix));
                        }
                        expansions += 1;

                        let target_base = if target.starts_with('/') {
                            Vec::new()
                        } else {
                            prefix_parts[..prefix_parts.len() - 1].to_vec()
                        };
                        let mut expanded = target_base;
                        for target_part in target.split('/') {
                            match target_part {
                                "" | "." => {}
                                ".." => {
                                    expanded.pop();
                                }
                                other => expanded.push(other.to_string()),
                            }
                        }
                        expanded.extend(parts[index + 1..].iter().cloned());
                        current = make_path(&expanded);
                        restarted = true;
                        break;
                    }
                    Some(Entry::Link(_)) => {}
                }
            }

            if !restarted {
                return Ok(current);
            }
        }
    }

    fn resolve_write(&self, path: &str) -> Result<String, Error> {
        if path == "/" {
            return Ok("/".to_string());
        }

        let mut current = path.to_string();
        let mut visited = HashSet::new();
        let mut expansions = 0usize;

        loop {
            let parts = components(&current);
            let mut prefix_parts: Vec<String> = Vec::new();
            let mut restarted = false;

            for (index, component) in parts.iter().enumerate() {
                prefix_parts.push(component.clone());
                let prefix = make_path(&prefix_parts);
                let is_final = index + 1 == parts.len();

                match self.entries.get(&prefix) {
                    None if is_final => return Ok(prefix),
                    None => return Err(Error::Missing(prefix)),
                    Some(Entry::Dir) => {}
                    Some(Entry::File(_)) => {
                        if !is_final {
                            return Err(Error::NotDir(prefix));
                        }
                    }
                    Some(Entry::Link(target)) => {
                        if !visited.insert(prefix.clone()) {
                            return Err(Error::Loop(prefix));
                        }
                        if expansions >= 8 {
                            return Err(Error::Depth(prefix));
                        }
                        expansions += 1;

                        let target_base = if target.starts_with('/') {
                            Vec::new()
                        } else {
                            prefix_parts[..prefix_parts.len() - 1].to_vec()
                        };
                        let mut expanded = target_base;
                        for target_part in target.split('/') {
                            match target_part {
                                "" | "." => {}
                                ".." => {
                                    expanded.pop();
                                }
                                other => expanded.push(other.to_string()),
                            }
                        }
                        expanded.extend(parts[index + 1..].iter().cloned());
                        current = make_path(&expanded);
                        restarted = true;
                        break;
                    }
                }
            }

            if !restarted {
                return Ok(current);
            }
        }
    }

    fn has_children(&self, path: &str) -> bool {
        self.entries.keys().any(|candidate| {
            candidate != path && is_proper_descendant(candidate, path)
        })
    }
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

fn make_path(components: &[String]) -> String {
    if components.is_empty() {
        "/".to_string()
    } else {
        format!("/{}", components.join("/"))
    }
}

fn split_path(path: &str) -> (String, String) {
    let index = path.rfind('/').expect("canonical absolute path");
    let name = path[index + 1..].to_string();
    let parent = if index == 0 {
        "/".to_string()
    } else {
        path[..index].to_string()
    };
    (parent, name)
}

fn join_path(parent: &str, name: &str) -> String {
    if parent == "/" {
        format!("/{name}")
    } else {
        format!("{parent}/{name}")
    }
}

fn is_within(path: &str, root: &str) -> bool {
    if root == "/" {
        return path.starts_with('/');
    }
    path == root
        || path
            .strip_prefix(root)
            .map(|suffix| suffix.starts_with('/'))
            .unwrap_or(false)
}

fn is_proper_descendant(path: &str, root: &str) -> bool {
    path != root && is_within(path, root)
}