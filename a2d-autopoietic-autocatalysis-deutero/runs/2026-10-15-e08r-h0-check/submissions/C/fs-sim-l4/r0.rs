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

fn canonical(parts: &[String]) -> String {
    if parts.is_empty() {
        "/".to_owned()
    } else {
        format!("/{}", parts.join("/"))
    }
}

fn components(path: &str) -> Vec<String> {
    path.split('/')
        .filter(|part| !part.is_empty())
        .map(str::to_owned)
        .collect()
}

fn normalize(path: &str, cwd: &str) -> Result<String, Error> {
    if let Some(i) = path.as_bytes().iter().position(|b| *b == 0) {
        return Err(Error::InvalidPath(i));
    }
    if path.is_empty() {
        return Err(Error::InvalidPath(0));
    }

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
            name => parts.push(name.to_owned()),
        }
    }
    Ok(canonical(&parts))
}

fn parent_name(path: &str) -> (String, String) {
    if path == "/" {
        return ("/".to_owned(), String::new());
    }
    let i = path.rfind('/').unwrap();
    let parent = if i == 0 { "/" } else { &path[..i] };
    (parent.to_owned(), path[i + 1..].to_owned())
}

fn join(parent: &str, name: &str) -> String {
    if parent == "/" {
        format!("/{name}")
    } else {
        format!("{parent}/{name}")
    }
}

fn within(path: &str, dir: &str) -> bool {
    path == dir || (dir == "/" && path.starts_with('/')) || path.starts_with(&format!("{dir}/"))
}

fn normalize_target(base: &str, target: &str, suffix: &[String]) -> String {
    let mut parts = if target.starts_with('/') {
        Vec::new()
    } else {
        components(base)
    };

    for part in target.split('/') {
        match part {
            "" | "." => {}
            ".." => {
                parts.pop();
            }
            name => parts.push(name.to_owned()),
        }
    }
    for part in suffix {
        match part.as_str() {
            "" | "." => {}
            ".." => {
                parts.pop();
            }
            name => parts.push(name.to_owned()),
        }
    }
    canonical(&parts)
}

fn resolve(
    entries: &BTreeMap<String, Entry>,
    path: &str,
    follow_final: bool,
    allow_missing_final: bool,
) -> Result<String, Error> {
    let mut todo = components(path);
    let mut resolved = Vec::<String>::new();
    let mut visited = HashSet::<String>::new();
    let mut expansions = 0usize;

    loop {
        if todo.is_empty() {
            return Ok(canonical(&resolved));
        }

        let name = todo.remove(0);
        let prefix = canonical(&resolved);
        let candidate = join(&prefix, &name);
        let is_final = todo.is_empty();

        match entries.get(&candidate) {
            None => {
                if is_final && allow_missing_final {
                    return Ok(candidate);
                }
                return Err(Error::Missing(candidate));
            }
            Some(Entry::Dir) => resolved.push(name),
            Some(Entry::File(_)) => {
                if !is_final {
                    return Err(Error::NotDir(candidate));
                }
                resolved.push(name);
            }
            Some(Entry::Link(target)) if !is_final || follow_final => {
                if !visited.insert(candidate.clone()) {
                    return Err(Error::Loop(candidate));
                }
                if expansions >= 8 {
                    return Err(Error::Depth(candidate));
                }
                expansions += 1;

                let expanded = normalize_target(&prefix, target, &todo);
                todo = components(&expanded);
                resolved.clear();
            }
            Some(Entry::Link(_)) => resolved.push(name),
        }
    }
}

fn resolve_parent(
    entries: &BTreeMap<String, Entry>,
    path: &str,
) -> Result<(String, String), Error> {
    let (parent, name) = parent_name(path);
    let physical_parent = resolve(entries, &parent, true, false)?;
    match entries.get(&physical_parent) {
        Some(Entry::Dir) => Ok((physical_parent, name)),
        Some(_) => Err(Error::NotDir(physical_parent)),
        None => Err(Error::Missing(physical_parent)),
    }
}

fn entry_size(entry: &Entry) -> u64 {
    match entry {
        Entry::Dir => 0,
        Entry::File(bytes) => bytes.len() as u64,
        Entry::Link(target) => target.len() as u64,
    }
}

fn subtree_size(entries: &BTreeMap<String, Entry>, path: &str) -> u64 {
    match entries.get(path) {
        Some(Entry::Dir) => entries
            .iter()
            .filter(|(p, _)| p.as_str() != path && within(p, path))
            .map(|(_, entry)| entry_size(entry))
            .sum(),
        Some(entry) => entry_size(entry),
        None => 0,
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

    pub fn mkdir(&mut self, path: &str) -> Result<(), Error> {
        let normalized = normalize(path, &self.cwd)?;
        if normalized == "/" {
            return Err(Error::Exists("/".to_owned()));
        }

        let (parent, name) = resolve_parent(&self.entries, &normalized)?;
        let destination = join(&parent, &name);
        if self.entries.contains_key(&destination) {
            return Err(Error::Exists(destination));
        }
        self.entries.insert(destination, Entry::Dir);
        Ok(())
    }

    pub fn write(&mut self, path: &str, data: &[u8]) -> Result<(), Error> {
        let normalized = normalize(path, &self.cwd)?;
        let resolved = resolve(&self.entries, &normalized, true, true)?;

        if resolved == "/" {
            return Err(Error::IsDir(resolved));
        }

        let (parent, name) = parent_name(&resolved);
        let physical_parent = resolve(&self.entries, &parent, true, false)?;
        match self.entries.get(&physical_parent) {
            Some(Entry::Dir) => {}
            Some(_) => return Err(Error::NotDir(physical_parent)),
            None => return Err(Error::Missing(physical_parent)),
        }

        match self.entries.get(&resolved) {
            Some(Entry::Dir) => return Err(Error::IsDir(resolved)),
            Some(Entry::File(_)) | None => {}
            Some(Entry::Link(_)) => return Err(Error::Loop(resolved)),
        }
        self.entries
            .insert(join(&physical_parent, &name), Entry::File(data.to_vec()));
        Ok(())
    }

    pub fn read(&self, path: &str) -> Result<Vec<u8>, Error> {
        let normalized = normalize(path, &self.cwd)?;
        let resolved = resolve(&self.entries, &normalized, true, false)?;
        match self.entries.get(&resolved) {
            Some(Entry::File(bytes)) => Ok(bytes.clone()),
            Some(Entry::Dir) => Err(Error::IsDir(resolved)),
            Some(Entry::Link(_)) => Err(Error::Loop(resolved)),
            None => Err(Error::Missing(resolved)),
        }
    }

    pub fn ls(&self, path: &str) -> Result<Vec<(String, Kind)>, Error> {
        let normalized = normalize(path, &self.cwd)?;
        let resolved = resolve(&self.entries, &normalized, true, false)?;
        match self.entries.get(&resolved) {
            Some(Entry::Dir) => {}
            Some(_) => return Err(Error::NotDir(resolved)),
            None => return Err(Error::Missing(resolved)),
        }

        let mut result = Vec::new();
        for (child_path, entry) in &self.entries {
            if child_path == "/" || child_path == &resolved || !within(child_path, &resolved) {
                continue;
            }
            let (parent, name) = parent_name(child_path);
            if parent == resolved {
                result.push((name, entry.kind()));
            }
        }
        result.sort_by(|a: &(String, Kind), b| a.0.cmp(&b.0));
        Ok(result)
    }

    pub fn cwd(&self) -> String {
        self.cwd.clone()
    }

    pub fn chdir(&mut self, path: &str) -> Result<(), Error> {
        let normalized = normalize(path, &self.cwd)?;
        let resolved = resolve(&self.entries, &normalized, true, false)?;
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
        let source_path = normalize(from, &self.cwd)?;
        let target_path = normalize(to, &self.cwd)?;
        if source_path == "/" || target_path == "/" {
            return Err(Error::Root);
        }

        let (source_parent, source_name) = resolve_parent(&self.entries, &source_path)?;
        let source = join(&source_parent, &source_name);
        let source_entry = self
            .entries
            .get(&source)
            .cloned()
            .ok_or_else(|| Error::Missing(source.clone()))?;

        if matches!(source_entry, Entry::Dir) && within(&self.cwd, &source) {
            return Err(Error::Busy(source));
        }

        let (target_parent, target_name) = resolve_parent(&self.entries, &target_path)?;
        let target = join(&target_parent, &target_name);
        if matches!(self.entries.get(&target), Some(Entry::Dir)) && within(&self.cwd, &target) {
            return Err(Error::Busy(target));
        }

        if source == target {
            return Ok(());
        }
        if matches!(source_entry, Entry::Dir) && within(&target, &source) {
            return Err(Error::Cycle(target));
        }

        let target_entry = self.entries.get(&target).cloned();
        if target_entry.is_some() && !overwrite {
            return Err(Error::Exists(target));
        }
        if let Some(destination) = &target_entry {
            match (&source_entry, destination) {
                (Entry::Dir, Entry::Dir) => {
                    if self.entries.keys().any(|p| p != &target && within(p, &target)) {
                        return Err(Error::NotEmpty(target));
                    }
                }
                (Entry::Dir, _) => return Err(Error::NotDir(target)),
                (_, Entry::Dir) => return Err(Error::IsDir(target)),
                _ => {}
            }
        }

        let moved: Vec<(String, Entry)> = self
            .entries
            .iter()
            .filter(|(p, _)| p.as_str() == source || within(p, &source))
            .map(|(p, entry)| (p.clone(), entry.clone()))
            .collect();

        if let Some(destination) = target_entry {
            if matches!(destination, Entry::Dir) {
                self.entries
                    .retain(|p, _| p != &target && !within(p, &target));
            } else {
                self.entries.remove(&target);
            }
        }
        self.entries
            .retain(|p, _| p != &source && !within(p, &source));

        for (old_path, entry) in moved {
            let suffix = &old_path[source.len()..];
            self.entries.insert(format!("{target}{suffix}"), entry);
        }
        Ok(())
    }

    pub fn remove(&mut self, path: &str, recursive: bool) -> Result<(), Error> {
        let normalized = normalize(path, &self.cwd)?;
        if normalized == "/" {
            return Err(Error::Root);
        }

        let (parent, name) = resolve_parent(&self.entries, &normalized)?;
        let resolved = join(&parent, &name);
        let entry = self
            .entries
            .get(&resolved)
            .cloned()
            .ok_or_else(|| Error::Missing(resolved.clone()))?;

        if matches!(entry, Entry::Dir) && within(&self.cwd, &resolved) {
            return Err(Error::Busy(resolved));
        }
        if matches!(entry, Entry::Dir)
            && !recursive
            && self.entries.keys().any(|p| p != &resolved && within(p, &resolved))
        {
            return Err(Error::NotEmpty(resolved));
        }

        self.entries
            .retain(|p, _| p != &resolved && !(recursive && within(p, &resolved)));
        Ok(())
    }

    pub fn symlink(&mut self, target: &str, link: &str) -> Result<(), Error> {
        let normalized = normalize(link, &self.cwd)?;
        if target.is_empty() {
            return Err(Error::InvalidTarget(0));
        }
        if let Some(i) = target.as_bytes().iter().position(|b| *b == 0) {
            return Err(Error::InvalidTarget(i));
        }

        if normalized == "/" {
            return Err(Error::Exists("/".to_owned()));
        }
        let (parent, name) = resolve_parent(&self.entries, &normalized)?;
        let resolved = join(&parent, &name);
        if self.entries.contains_key(&resolved) {
            return Err(Error::Exists(resolved));
        }
        self.entries.insert(resolved, Entry::Link(target.to_owned()));
        Ok(())
    }

    pub fn read_link(&self, path: &str) -> Result<String, Error> {
        let normalized = normalize(path, &self.cwd)?;
        if normalized == "/" {
            return Err(Error::InvalidTarget(0));
        }

        let (parent, name) = parent_name(&normalized);
        let resolved_parent = resolve(&self.entries, &parent, true, false)?;
        let final_path = join(&resolved_parent, &name);
        match self.entries.get(&final_path) {
            Some(Entry::Link(target)) => Ok(target.clone()),
            Some(_) => Err(Error::InvalidTarget(0)),
            None => Err(Error::Missing(final_path)),
        }
    }

    pub fn du(&self, path: &str) -> Result<Vec<(String, u64)>, Error> {
        let normalized = normalize(path, &self.cwd)?;
        let resolved = resolve(&self.entries, &normalized, true, false)?;

        let mut paths: Vec<String> = self
            .entries
            .keys()
            .filter(|p| p.as_str() == resolved || within(p, &resolved))
            .cloned()
            .collect();
        paths.sort();

        Ok(paths
            .into_iter()
            .map(|entry_path| {
                let size = subtree_size(&self.entries, &entry_path);
                (entry_path, size)
            })
            .collect())
    }
}

fn main() {}