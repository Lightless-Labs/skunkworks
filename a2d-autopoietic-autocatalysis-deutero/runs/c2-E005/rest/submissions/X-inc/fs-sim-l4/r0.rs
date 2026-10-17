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
        .map(str::to_owned)
        .collect()
}

fn canonical(parts: &[String]) -> String {
    if parts.is_empty() {
        "/".to_owned()
    } else {
        format!("/{}", parts.join("/"))
    }
}

fn normalize(path: &str, cwd: &str) -> String {
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
            other => parts.push(other.to_owned()),
        }
    }

    canonical(&parts)
}

fn parent_and_name(path: &str) -> (String, String) {
    if path == "/" {
        return ("/".to_owned(), String::new());
    }
    let idx = path.rfind('/').unwrap_or(0);
    let parent = if idx == 0 {
        "/".to_owned()
    } else {
        path[..idx].to_owned()
    };
    (parent, path[idx + 1..].to_owned())
}

fn join(parent: &str, name: &str) -> String {
    if parent == "/" {
        format!("/{name}")
    } else {
        format!("{parent}/{name}")
    }
}

fn is_within(path: &str, dir: &str) -> bool {
    if dir == "/" {
        path.starts_with('/')
    } else {
        path == dir || path.strip_prefix(dir).is_some_and(|suffix| suffix.starts_with('/'))
    }
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

    fn kind_of(entry: &Entry) -> Kind {
        match entry {
            Entry::Dir => Kind::Dir,
            Entry::File(_) => Kind::File,
            Entry::Link(_) => Kind::Link,
        }
    }

    fn resolve(&self, path: &str, allow_missing_final: bool) -> Result<String, Error> {
        let mut pending = components(path);
        let mut physical = Vec::<String>::new();
        let mut visited = BTreeSet::<String>::new();
        let mut expansions = 0usize;

        while !pending.is_empty() {
            let component = pending.remove(0);
            let candidate_parts = {
                let mut p = physical.clone();
                p.push(component.clone());
                p
            };
            let candidate = canonical(&candidate_parts);
            let is_final = pending.is_empty();

            match self.entries.get(&candidate) {
                None if is_final && allow_missing_final => return Ok(candidate),
                None => return Err(Error::Missing(candidate)),
                Some(Entry::Dir) => physical.push(component),
                Some(Entry::File(_)) if !is_final => return Err(Error::NotDir(candidate)),
                Some(Entry::File(_)) => return Ok(candidate),
                Some(Entry::Link(target)) => {
                    if !visited.insert(candidate.clone()) {
                        return Err(Error::Loop(candidate));
                    }
                    if expansions == 8 {
                        return Err(Error::Depth(candidate));
                    }
                    expansions += 1;

                    let (link_parent, _) = parent_and_name(&candidate);
                    let mut replacement = if target.starts_with('/') {
                        Vec::new()
                    } else {
                        components(&link_parent)
                    };

                    for part in target.split('/') {
                        match part {
                            "" | "." => {}
                            ".." => {
                                replacement.pop();
                            }
                            other => replacement.push(other.to_owned()),
                        }
                    }

                    let mut expanded = components(&canonical(&replacement));
                    expanded.extend(pending.into_iter());
                    pending = expanded;
                    physical.clear();
                }
            }
        }

        Ok(canonical(&physical))
    }

    fn resolve_parent(&self, path: &str) -> Result<(String, String), Error> {
        if path == "/" {
            return Ok(("/".to_owned(), String::new()));
        }
        let (parent, name) = parent_and_name(path);
        let resolved_parent = self.resolve(&parent, false)?;
        match self.entries.get(&resolved_parent) {
            Some(Entry::Dir) => Ok((resolved_parent, name)),
            Some(_) => Err(Error::NotDir(resolved_parent)),
            None => Err(Error::Missing(resolved_parent)),
        }
    }

    fn child_paths(&self, dir: &str) -> Vec<String> {
        self.entries
            .keys()
            .filter(|path| {
                if dir == "/" {
                    path.starts_with('/') && *path != "/" && !path[1..].contains('/')
                } else {
                    path.strip_prefix(dir)
                        .is_some_and(|suffix| suffix.starts_with('/') && !suffix[1..].contains('/'))
                }
            })
            .cloned()
            .collect()
    }

    pub fn mkdir(&mut self, path: &str) -> Result<(), Error> {
        validate(path)?;
        let normalized = normalize(path, &self.cwd);
        if normalized == "/" {
            return Err(Error::Exists("/".to_owned()));
        }

        let (parent, name) = self.resolve_parent(&normalized)?;
        let destination = join(&parent, &name);
        if self.entries.contains_key(&destination) {
            return Err(Error::Exists(destination));
        }
        self.entries.insert(destination, Entry::Dir);
        Ok(())
    }

    pub fn write(&mut self, path: &str, data: &[u8]) -> Result<(), Error> {
        validate(path)?;
        let normalized = normalize(path, &self.cwd);
        let resolved = self.resolve(&normalized, true)?;

        if resolved == "/" {
            return Err(Error::IsDir("/".to_owned()));
        }

        if let Some(entry) = self.entries.get(&resolved) {
            if matches!(entry, Entry::Dir) {
                return Err(Error::IsDir(resolved));
            }
            self.entries.insert(resolved, Entry::File(data.to_vec()));
            return Ok(());
        }

        let (parent, name) = self.resolve_parent(&resolved)?;
        let destination = join(&parent, &name);
        self.entries.insert(destination, Entry::File(data.to_vec()));
        Ok(())
    }

    pub fn read(&self, path: &str) -> Result<Vec<u8>, Error> {
        validate(path)?;
        let normalized = normalize(path, &self.cwd);
        let resolved = self.resolve(&normalized, false)?;
        match self.entries.get(&resolved) {
            Some(Entry::File(data)) => Ok(data.clone()),
            Some(Entry::Dir) => Err(Error::IsDir(resolved)),
            Some(Entry::Link(_)) | None => Err(Error::Missing(resolved)),
        }
    }

    pub fn ls(&self, path: &str) -> Result<Vec<(String, Kind)>, Error> {
        validate(path)?;
        let normalized = normalize(path, &self.cwd);
        let resolved = self.resolve(&normalized, false)?;
        match self.entries.get(&resolved) {
            Some(Entry::Dir) => {
                let mut rows: Vec<(String, Kind)> = self
                    .child_paths(&resolved)
                    .into_iter()
                    .filter_map(|child| {
                        self.entries.get(&child).map(|entry| {
                            let (_, name) = parent_and_name(&child);
                            (name, Self::kind_of(entry))
                        })
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
        validate(path)?;
        let normalized = normalize(path, &self.cwd);
        let resolved = self.resolve(&normalized, false)?;
        match self.entries.get(&resolved) {
            Some(Entry::Dir) => {
                self.cwd = resolved;
                Ok(())
            }
            _ => Err(Error::NotDir(resolved)),
        }
    }

    pub fn rename(&mut self, from: &str, to: &str, overwrite: bool) -> Result<(), Error> {
        validate(from)?;
        let source = normalize(from, &self.cwd);
        validate(to)?;
        let destination = normalize(to, &self.cwd);

        if source == "/" || destination == "/" {
            return Err(Error::Root);
        }

        let (src_parent, src_name) = self.resolve_parent(&source)?;
        let source_path = join(&src_parent, &src_name);
        let source_entry = self
            .entries
            .get(&source_path)
            .cloned()
            .ok_or_else(|| Error::Missing(source_path.clone()))?;

        if matches!(source_entry, Entry::Dir) && is_within(&self.cwd, &source_path) {
            return Err(Error::Busy(source_path));
        }

        let (dst_parent, dst_name) = self.resolve_parent(&destination)?;
        let destination_path = join(&dst_parent, &dst_name);

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

        if self.entries.contains_key(&destination_path) && !overwrite {
            return Err(Error::Exists(destination_path));
        }

        if let Some(dest_entry) = self.entries.get(&destination_path) {
            match (&source_entry, dest_entry) {
                (Entry::Dir, Entry::Dir) => {
                    if self.child_paths(&destination_path).len() > 0 {
                        return Err(Error::NotEmpty(destination_path));
                    }
                }
                (Entry::Dir, _) => return Err(Error::NotDir(destination_path)),
                (_, Entry::Dir) => return Err(Error::IsDir(destination_path)),
                _ => {}
            }
        }

        let moving: Vec<(String, Entry)> = self
            .entries
            .iter()
            .filter(|(path, _)| is_within(path, &source_path))
            .map(|(path, entry)| (path.clone(), entry.clone()))
            .collect();

        let replacing: Vec<String> = self
            .entries
            .keys()
            .filter(|path| is_within(path, &destination_path))
            .cloned()
            .collect();

        for path in replacing {
            self.entries.remove(&path);
        }
        for (path, _) in &moving {
            self.entries.remove(path);
        }
        for (old_path, entry) in moving {
            let suffix = &old_path[source_path.len()..];
            self.entries.insert(format!("{destination_path}{suffix}"), entry);
        }
        Ok(())
    }

    pub fn remove(&mut self, path: &str, recursive: bool) -> Result<(), Error> {
        validate(path)?;
        let normalized = normalize(path, &self.cwd);
        if normalized == "/" {
            return Err(Error::Root);
        }

        let (parent, name) = self.resolve_parent(&normalized)?;
        let target = join(&parent, &name);
        let entry = self
            .entries
            .get(&target)
            .cloned()
            .ok_or_else(|| Error::Missing(target.clone()))?;

        if matches!(entry, Entry::Dir) && is_within(&self.cwd, &target) {
            return Err(Error::Busy(target));
        }

        let descendants = self.child_paths(&target);
        if matches!(entry, Entry::Dir) && !recursive && !descendants.is_empty() {
            return Err(Error::NotEmpty(target));
        }

        let to_remove: Vec<String> = self
            .entries
            .keys()
            .filter(|p| is_within(p, &target))
            .cloned()
            .collect();
        for p in to_remove {
            self.entries.remove(&p);
        }
        Ok(())
    }

    pub fn symlink(&mut self, target: &str, link: &str) -> Result<(), Error> {
        validate(link)?;
        let normalized_link = normalize(link, &self.cwd);

        if target.is_empty() {
            return Err(Error::InvalidTarget(0));
        }
        if let Some(i) = target.as_bytes().iter().position(|&b| b == 0) {
            return Err(Error::InvalidTarget(i));
        }

        if normalized_link == "/" {
            return Err(Error::Exists("/".to_owned()));
        }

        let (parent, name) = self.resolve_parent(&normalized_link)?;
        let link_path = join(&parent, &name);
        if self.entries.contains_key(&link_path) {
            return Err(Error::Exists(link_path));
        }

        self.entries.insert(link_path, Entry::Link(target.to_owned()));
        Ok(())
    }

    pub fn read_link(&self, path: &str) -> Result<String, Error> {
        validate(path)?;
        let normalized = normalize(path, &self.cwd);
        if normalized == "/" {
            return Err(Error::InvalidTarget(0));
        }

        let (parent, name) = self.resolve_parent(&normalized)?;
        let final_path = join(&parent, &name);
        match self.entries.get(&final_path) {
            Some(Entry::Link(target)) => Ok(target.clone()),
            Some(_) => Err(Error::InvalidTarget(0)),
            None => Err(Error::Missing(final_path)),
        }
    }

    pub fn du(&self, path: &str) -> Result<Vec<(String, u64)>, Error> {
        validate(path)?;
        let normalized = normalize(path, &self.cwd);
        let resolved = self.resolve(&normalized, false)?;
        let root_entry = self
            .entries
            .get(&resolved)
            .ok_or_else(|| Error::Missing(resolved.clone()))?;

        let mut paths = vec![resolved.clone()];
        if matches!(root_entry, Entry::Dir) {
            paths.extend(
                self.entries
                    .keys()
                    .filter(|candidate| *candidate != &resolved && is_within(candidate, &resolved))
                    .cloned(),
            );
        }
        paths.sort();

        let mut sizes = BTreeMap::<String, u64>::new();
        for item in paths.iter().rev() {
            let size = match self.entries.get(item) {
                Some(Entry::File(data)) => data.len() as u64,
                Some(Entry::Link(target)) => target.len() as u64,
                Some(Entry::Dir) => {
                    self.child_paths(item)
                        .iter()
                        .map(|child| sizes.get(child).copied().unwrap_or(0))
                        .sum()
                }
                None => 0,
            };
            sizes.insert(item.clone(), size);
        }

        Ok(paths
            .into_iter()
            .map(|item| {
                let size = sizes.get(&item).copied().unwrap_or(0);
                (item, size)
            })
            .collect())
    }
}

impl Default for Fs {
    fn default() -> Self {
        Self::new()
    }
}