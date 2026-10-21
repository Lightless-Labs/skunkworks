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

fn validate_path(path: &str) -> Result<(), Error> {
    if let Some(i) = path.as_bytes().iter().position(|b| *b == 0) {
        return Err(Error::InvalidPath(i));
    }
    if path.is_empty() {
        return Err(Error::InvalidPath(0));
    }
    Ok(())
}

fn normalize_components<I, S>(base: &[String], parts: I) -> Vec<String>
where
    I: IntoIterator<Item = S>,
    S: AsRef<str>,
{
    let mut result = base.to_vec();
    for part in parts {
        match part.as_ref() {
            "" | "." => {}
            ".." => {
                result.pop();
            }
            name => result.push(name.to_owned()),
        }
    }
    result
}

fn path_of(parts: &[String]) -> String {
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
    validate_path(path)?;
    let base = if path.starts_with('/') {
        Vec::new()
    } else {
        components(cwd)
    };
    Ok(path_of(&normalize_components(&base, path.split('/'))))
}

fn parent_path(path: &str) -> String {
    let mut parts = components(path);
    parts.pop();
    path_of(&parts)
}

fn is_within(path: &str, directory: &str) -> bool {
    path == directory
        || (directory == "/" && path.starts_with('/'))
        || path
            .strip_prefix(directory)
            .is_some_and(|rest| rest.starts_with('/'))
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

    fn resolve(&self, start: &str, follow_final: bool, create_final: bool) -> Result<String, Error> {
        let mut pending = components(start);
        let mut physical: Vec<String> = Vec::new();
        let mut visited = BTreeSet::new();
        let mut expansions = 0usize;

        loop {
            if pending.is_empty() {
                return Ok(path_of(&physical));
            }

            let name = pending.remove(0);
            let candidate_parts = {
                let mut p = physical.clone();
                p.push(name.clone());
                p
            };
            let candidate = path_of(&candidate_parts);
            let final_component = pending.is_empty();

            let Some(entry) = self.entries.get(&candidate) else {
                if create_final && final_component {
                    return Ok(candidate);
                }
                return Err(Error::Missing(candidate));
            };

            let should_follow = matches!(entry, Entry::Link(_)) && (!final_component || follow_final);
            if should_follow {
                if !visited.insert(candidate.clone()) {
                    return Err(Error::Loop(candidate));
                }
                if expansions >= 8 {
                    return Err(Error::Depth(candidate));
                }
                expansions += 1;

                let Entry::Link(target) = entry else {
                    unreachable!()
                };
                let target_base = if target.starts_with('/') {
                    Vec::new()
                } else {
                    physical.clone()
                };
                let mut expanded = normalize_components(&target_base, target.split('/'));
                expanded.extend(pending.into_iter());
                pending = expanded;
                physical.clear();
                continue;
            }

            if !final_component && !matches!(entry, Entry::Dir) {
                return Err(Error::NotDir(candidate));
            }

            physical.push(name);
        }
    }

    fn resolve_parent(&self, path: &str) -> Result<(String, String), Error> {
        if path == "/" {
            return Ok(("/".to_owned(), "/".to_owned()));
        }
        let parent = parent_path(path);
        let name = components(path).last().cloned().unwrap_or_default();
        let resolved_parent = self.resolve(&parent, true, false)?;
        match self.entries.get(&resolved_parent) {
            Some(Entry::Dir) => Ok((resolved_parent, name)),
            Some(_) => Err(Error::NotDir(resolved_parent)),
            None => Err(Error::Missing(resolved_parent)),
        }
    }

    fn resolved_entry_path(&self, path: &str, follow_final: bool) -> Result<String, Error> {
        self.resolve(path, follow_final, false)
    }

    pub fn mkdir(&mut self, path: &str) -> Result<(), Error> {
        let normalized = normalize(path, &self.cwd)?;
        if normalized == "/" {
            return Err(Error::Exists("/".to_owned()));
        }
        let (parent, name) = self.resolve_parent(&normalized)?;
        let final_path = if parent == "/" {
            format!("/{name}")
        } else {
            format!("{parent}/{name}")
        };
        if self.entries.contains_key(&final_path) {
            return Err(Error::Exists(final_path));
        }
        self.entries.insert(final_path, Entry::Dir);
        Ok(())
    }

    pub fn write(&mut self, path: &str, data: &[u8]) -> Result<(), Error> {
        let normalized = normalize(path, &self.cwd)?;
        let resolved = self.resolve(&normalized, true, true)?;
        if resolved == "/" {
            return Err(Error::IsDir("/".to_owned()));
        }
        if let Some(entry) = self.entries.get(&resolved) {
            match entry {
                Entry::Dir => return Err(Error::IsDir(resolved)),
                Entry::File(_) => {}
                Entry::Link(_) => return Err(Error::IsDir(resolved)),
            }
        } else {
            let (parent, _) = self.resolve_parent(&resolved)?;
            let _ = parent;
            self.entries.insert(resolved.clone(), Entry::File(data.to_vec()));
            return Ok(());
        }
        self.entries.insert(resolved, Entry::File(data.to_vec()));
        Ok(())
    }

    pub fn read(&self, path: &str) -> Result<Vec<u8>, Error> {
        let normalized = normalize(path, &self.cwd)?;
        let resolved = self.resolve(&normalized, true, false)?;
        match self.entries.get(&resolved) {
            Some(Entry::File(data)) => Ok(data.clone()),
            Some(Entry::Dir) => Err(Error::IsDir(resolved)),
            Some(Entry::Link(_)) => Err(Error::Missing(resolved)),
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
        for (entry_path, entry) in &self.entries {
            if entry_path == &resolved {
                continue;
            }
            if parent_path(entry_path) == resolved {
                let name = components(entry_path).last().cloned().unwrap_or_default();
                result.push((name, entry.kind()));
            }
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
        let from = normalize(from, &self.cwd)?;
        let to = normalize(to, &self.cwd)?;
        if from == "/" || to == "/" {
            return Err(Error::Root);
        }

        let (src_parent, src_name) = self.resolve_parent(&from)?;
        let source = if src_parent == "/" {
            format!("/{src_name}")
        } else {
            format!("{src_parent}/{src_name}")
        };
        let src_entry = self
            .entries
            .get(&source)
            .cloned()
            .ok_or_else(|| Error::Missing(source.clone()))?;

        if matches!(src_entry, Entry::Dir) && is_within(&self.cwd, &source) {
            return Err(Error::Busy(source));
        }

        let (dst_parent, dst_name) = self.resolve_parent(&to)?;
        let destination = if dst_parent == "/" {
            format!("/{dst_name}")
        } else {
            format!("{dst_parent}/{dst_name}")
        };

        if matches!(self.entries.get(&destination), Some(Entry::Dir))
            && is_within(&self.cwd, &destination)
        {
            return Err(Error::Busy(destination));
        }

        if source == destination {
            return Ok(());
        }

        if matches!(src_entry, Entry::Dir) && is_within(&destination, &source) {
            return Err(Error::Cycle(destination));
        }

        let dst_entry = self.entries.get(&destination).cloned();
        if dst_entry.is_some() && !overwrite {
            return Err(Error::Exists(destination));
        }

        if overwrite {
            if let Some(dst) = &dst_entry {
                match (&src_entry, dst) {
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
        }

        let moved: Vec<(String, Entry)> = self
            .entries
            .iter()
            .filter(|(p, _)| is_within(p, &source))
            .map(|(p, e)| (p.clone(), e.clone()))
            .collect();

        if let Some(dst) = dst_entry {
            if matches!(dst, Entry::Dir) {
                let keys: Vec<String> = self
                    .entries
                    .keys()
                    .filter(|p| is_within(p, &destination))
                    .cloned()
                    .collect();
                for key in keys {
                    self.entries.remove(&key);
                }
            } else {
                self.entries.remove(&destination);
            }
        }

        let keys: Vec<String> = moved.iter().map(|(p, _)| p.clone()).collect();
        for key in keys {
            self.entries.remove(&key);
        }

        for (old, entry) in moved {
            let suffix = old.strip_prefix(&source).unwrap_or("");
            let new_path = format!("{destination}{suffix}");
            self.entries.insert(new_path, entry);
        }
        Ok(())
    }

    fn has_children(&self, path: &str) -> bool {
        self.entries
            .keys()
            .any(|p| p != path && is_within(p, path))
    }

    pub fn remove(&mut self, path: &str, recursive: bool) -> Result<(), Error> {
        let normalized = normalize(path, &self.cwd)?;
        if normalized == "/" {
            return Err(Error::Root);
        }

        let (parent, name) = self.resolve_parent(&normalized)?;
        let target = if parent == "/" {
            format!("/{name}")
        } else {
            format!("{parent}/{name}")
        };
        let entry = self
            .entries
            .get(&target)
            .cloned()
            .ok_or_else(|| Error::Missing(target.clone()))?;

        if matches!(entry, Entry::Dir) && is_within(&self.cwd, &target) {
            return Err(Error::Busy(target));
        }

        if matches!(entry, Entry::Dir) && self.has_children(&target) && !recursive {
            return Err(Error::NotEmpty(target));
        }

        let keys: Vec<String> = self
            .entries
            .keys()
            .filter(|p| *p == &target || (recursive && is_within(p, &target)))
            .cloned()
            .collect();
        for key in keys {
            self.entries.remove(&key);
        }
        Ok(())
    }

    pub fn symlink(&mut self, target: &str, link: &str) -> Result<(), Error> {
        let normalized = normalize(link, &self.cwd)?;
        if let Some(i) = target.as_bytes().iter().position(|b| *b == 0) {
            return Err(Error::InvalidTarget(i));
        }
        if target.is_empty() {
            return Err(Error::InvalidTarget(0));
        }
        if normalized == "/" {
            return Err(Error::Exists("/".to_owned()));
        }
        let (parent, name) = self.resolve_parent(&normalized)?;
        let final_path = if parent == "/" {
            format!("/{name}")
        } else {
            format!("{parent}/{name}")
        };
        if self.entries.contains_key(&final_path) {
            return Err(Error::Exists(final_path));
        }
        self.entries
            .insert(final_path, Entry::Link(target.to_owned()));
        Ok(())
    }

    pub fn read_link(&self, path: &str) -> Result<String, Error> {
        let normalized = normalize(path, &self.cwd)?;
        let resolved = self.resolve(&normalized, false, false)?;
        match self.entries.get(&resolved) {
            Some(Entry::Link(target)) => Ok(target.clone()),
            Some(_) => Err(Error::InvalidTarget(0)),
            None => Err(Error::Missing(resolved)),
        }
    }

    pub fn du(&self, path: &str) -> Result<Vec<(String, u64)>, Error> {
        let normalized = normalize(path, &self.cwd)?;
        let resolved = self.resolve(&normalized, true, false)?;
        let Some(root_entry) = self.entries.get(&resolved) else {
            return Err(Error::Missing(resolved));
        };

        let mut rows = Vec::new();
        let mut subtree: Vec<(String, &Entry)> = self
            .entries
            .iter()
            .filter(|(p, _)| is_within(p, &resolved))
            .map(|(p, e)| (p.clone(), e))
            .collect();

        if subtree.is_empty() {
            subtree.push((resolved.clone(), root_entry));
        }

        for (entry_path, entry) in &subtree {
            let size = match entry {
                Entry::File(data) => data.len() as u64,
                Entry::Link(target) => target.len() as u64,
                Entry::Dir => subtree
                    .iter()
                    .filter(|(p, e)| {
                        p != entry_path
                            && is_within(p, entry_path)
                            && !matches!(e, Entry::Dir)
                    })
                    .map(|(_, e)| match e {
                        Entry::File(data) => data.len() as u64,
                        Entry::Link(target) => target.len() as u64,
                        Entry::Dir => 0,
                    })
                    .sum(),
            };
            rows.push((entry_path.clone(), size));
        }
        rows.sort_by(|a, b| a.0.cmp(&b.0));
        Ok(rows)
    }
}