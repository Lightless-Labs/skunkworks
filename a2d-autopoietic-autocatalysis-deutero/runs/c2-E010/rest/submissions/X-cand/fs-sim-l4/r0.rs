use std::collections::{BTreeMap, HashSet};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind { Dir, File, Link }

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

fn components(path: &str) -> Result<Vec<String>, Error> {
    if let Some(i) = path.as_bytes().iter().position(|&b| b == 0) {
        return Err(Error::InvalidPath(i));
    }
    if path.is_empty() {
        return Err(Error::InvalidPath(0));
    }
    let mut out = Vec::new();
    for part in path.split('/') {
        match part {
            "" | "." => {}
            ".." => { out.pop(); }
            _ => out.push(part.to_owned()),
        }
    }
    Ok(out)
}

fn path_of(parts: &[String]) -> String {
    if parts.is_empty() { "/".to_owned() } else { format!("/{}", parts.join("/")) }
}

fn split_path(path: &str) -> Vec<String> {
    if path == "/" { Vec::new() }
    else { path[1..].split('/').map(str::to_owned).collect() }
}

fn is_within(path: &str, dir: &str) -> bool {
    if dir == "/" { return true; }
    path == dir || path.strip_prefix(dir).is_some_and(|s| s.starts_with('/'))
}

impl Fs {
    pub fn new() -> Self {
        let mut entries = BTreeMap::new();
        entries.insert("/".to_owned(), Entry::Dir);
        Self { entries, cwd: "/".to_owned() }
    }

    fn normalize(&self, path: &str) -> Result<String, Error> {
        let mut parts = if path.starts_with('/') { Vec::new() } else { split_path(&self.cwd) };
        if let Some(i) = path.as_bytes().iter().position(|&b| b == 0) {
            return Err(Error::InvalidPath(i));
        }
        if path.is_empty() { return Err(Error::InvalidPath(0)); }
        for part in path.split('/') {
            match part {
                "" | "." => {}
                ".." => { parts.pop(); }
                _ => parts.push(part.to_owned()),
            }
        }
        Ok(path_of(&parts))
    }

    fn resolve(&self, initial: &str, allow_missing_final: bool) -> Result<String, Error> {
        let mut todo = split_path(initial);
        let mut built: Vec<String> = Vec::new();
        let mut visited = HashSet::new();
        let mut expansions = 0usize;
        let mut index = 0usize;

        while index < todo.len() {
            let part = todo[index].clone();
            built.push(part);
            let current = path_of(&built);
            let final_component = index + 1 == todo.len();

            let entry = match self.entries.get(&current) {
                Some(e) => e,
                None if final_component && allow_missing_final => return Ok(current),
                None => return Err(Error::Missing(current)),
            };

            if let Entry::Link(target) = entry {
                if !visited.insert(current.clone()) {
                    return Err(Error::Loop(current));
                }
                if expansions >= 8 {
                    return Err(Error::Depth(current));
                }
                expansions += 1;

                let mut target_parts = if target.starts_with('/') {
                    Vec::new()
                } else {
                    built[..built.len() - 1].to_vec()
                };
                for component in target.split('/') {
                    match component {
                        "" | "." => {}
                        ".." => { target_parts.pop(); }
                        _ => target_parts.push(component.to_owned()),
                    }
                }
                target_parts.extend(todo[index + 1..].iter().cloned());
                let expanded = path_of(&target_parts);
                todo = split_path(&expanded);
                built.clear();
                index = 0;
                continue;
            }

            if !final_component && !matches!(entry, Entry::Dir) {
                return Err(Error::NotDir(current));
            }
            index += 1;
        }
        Ok(path_of(&built))
    }

    fn resolve_parent(&self, path: &str) -> Result<(String, String), Error> {
        let mut parts = split_path(path);
        let name = parts.pop().unwrap_or_default();
        let parent = path_of(&parts);
        let resolved_parent = self.resolve(&parent, false)?;
        if !matches!(self.entries.get(&resolved_parent), Some(Entry::Dir)) {
            return Err(Error::NotDir(resolved_parent));
        }
        Ok((resolved_parent, name))
    }

    fn child_paths(&self, path: &str) -> Vec<String> {
        self.entries.keys()
            .filter(|p| *p != path && is_within(p, path))
            .filter(|p| {
                let remainder = if path == "/" { &p[1..] } else { &p[path.len() + 1..] };
                !remainder.is_empty() && !remainder.contains('/')
            })
            .cloned()
            .collect()
    }

    pub fn mkdir(&mut self, path: &str) -> Result<(), Error> {
        let canonical = self.normalize(path)?;
        let (parent, name) = self.resolve_parent(&canonical)?;
        let resolved = if parent == "/" { format!("/{}", name) } else { format!("{}/{}", parent, name) };
        if self.entries.contains_key(&resolved) {
            return Err(Error::Exists(resolved));
        }
        self.entries.insert(resolved, Entry::Dir);
        Ok(())
    }

    pub fn write(&mut self, path: &str, data: &[u8]) -> Result<(), Error> {
        let canonical = self.normalize(path)?;
        let resolved = self.resolve(&canonical, true)?;
        if matches!(self.entries.get(&resolved), Some(Entry::Dir)) {
            return Err(Error::IsDir(resolved));
        }
        if !self.entries.contains_key(&resolved) {
            let (parent, _) = self.resolve_parent(&resolved)?;
            let _ = parent;
        }
        self.entries.insert(resolved, Entry::File(data.to_vec()));
        Ok(())
    }

    pub fn read(&self, path: &str) -> Result<Vec<u8>, Error> {
        let canonical = self.normalize(path)?;
        let resolved = self.resolve(&canonical, false)?;
        match self.entries.get(&resolved) {
            Some(Entry::File(bytes)) => Ok(bytes.clone()),
            Some(Entry::Dir) => Err(Error::IsDir(resolved)),
            Some(Entry::Link(_)) => unreachable!(),
            None => Err(Error::Missing(resolved)),
        }
    }

    pub fn ls(&self, path: &str) -> Result<Vec<(String, Kind)>, Error> {
        let canonical = self.normalize(path)?;
        let resolved = self.resolve(&canonical, false)?;
        if !matches!(self.entries.get(&resolved), Some(Entry::Dir)) {
            return Err(Error::NotDir(resolved));
        }
        let mut result = Vec::new();
        for child in self.child_paths(&resolved) {
            let basename = child.rsplit('/').next().unwrap_or("").to_owned();
            if let Some(entry) = self.entries.get(&child) {
                result.push((basename, entry.kind()));
            }
        }
        result.sort_by(|a, b| a.0.cmp(&b.0));
        Ok(result)
    }

    pub fn cwd(&self) -> String { self.cwd.clone() }

    pub fn chdir(&mut self, path: &str) -> Result<(), Error> {
        let canonical = self.normalize(path)?;
        let resolved = self.resolve(&canonical, false)?;
        if !matches!(self.entries.get(&resolved), Some(Entry::Dir)) {
            return Err(Error::NotDir(resolved));
        }
        self.cwd = resolved;
        Ok(())
    }

    pub fn rename(&mut self, from: &str, to: &str, overwrite: bool) -> Result<(), Error> {
        let src = self.normalize(from)?;
        let dst = self.normalize(to)?;
        if src == "/" || dst == "/" { return Err(Error::Root); }

        let (src_parent, src_name) = self.resolve_parent(&src)?;
        let source = if src_parent == "/" { format!("/{}", src_name) } else { format!("{}/{}", src_parent, src_name) };
        let source_entry = self.entries.get(&source).cloned().ok_or_else(|| Error::Missing(source.clone()))?;

        if matches!(source_entry, Entry::Dir) && is_within(&self.cwd, &source) {
            return Err(Error::Busy(source));
        }

        let (dst_parent, dst_name) = self.resolve_parent(&dst)?;
        let destination = if dst_parent == "/" { format!("/{}", dst_name) } else { format!("{}/{}", dst_parent, dst_name) };

        if matches!(self.entries.get(&destination), Some(Entry::Dir)) && is_within(&self.cwd, &destination) {
            return Err(Error::Busy(destination));
        }
        if source == destination { return Ok(()); }
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
                    if self.child_paths(&destination).len() > 0 {
                        return Err(Error::NotEmpty(destination));
                    }
                }
                (Entry::Dir, _) => return Err(Error::NotDir(destination)),
                (_, Entry::Dir) => return Err(Error::IsDir(destination)),
                _ => {}
            }
        }

        let moving: Vec<(String, Entry)> = self.entries.iter()
            .filter(|(p, _)| is_within(p, &source))
            .map(|(p, e)| (p.clone(), e.clone()))
            .collect();
        for (p, _) in &moving { self.entries.remove(p); }
        if destination_entry.is_some() { self.entries.remove(&destination); }
        for (old, entry) in moving {
            let suffix = if old == source { String::new() } else { old[source.len()..].to_owned() };
            self.entries.insert(format!("{}{}", destination, suffix), entry);
        }
        Ok(())
    }

    pub fn remove(&mut self, path: &str, recursive: bool) -> Result<(), Error> {
        let canonical = self.normalize(path)?;
        if canonical == "/" { return Err(Error::Root); }
        let (parent, name) = self.resolve_parent(&canonical)?;
        let resolved = if parent == "/" { format!("/{}", name) } else { format!("{}/{}", parent, name) };
        let entry = self.entries.get(&resolved).cloned().ok_or_else(|| Error::Missing(resolved.clone()))?;

        if matches!(entry, Entry::Dir) && is_within(&self.cwd, &resolved) {
            return Err(Error::Busy(resolved));
        }
        if matches!(entry, Entry::Dir) && !self.child_paths(&resolved).is_empty() && !recursive {
            return Err(Error::NotEmpty(resolved));
        }
        if matches!(entry, Entry::Dir) && recursive {
            let remove: Vec<String> = self.entries.keys()
                .filter(|p| is_within(p, &resolved))
                .cloned().collect();
            for p in remove { self.entries.remove(&p); }
        } else {
            self.entries.remove(&resolved);
        }
        Ok(())
    }

    pub fn symlink(&mut self, target: &str, link: &str) -> Result<(), Error> {
        let canonical_link = self.normalize(link)?;
        if target.is_empty() { return Err(Error::InvalidTarget(0)); }
        if let Some(i) = target.as_bytes().iter().position(|&b| b == 0) {
            return Err(Error::InvalidTarget(i));
        }
        if canonical_link == "/" { return Err(Error::Exists("/".to_owned())); }
        let (parent, name) = self.resolve_parent(&canonical_link)?;
        let resolved = if parent == "/" { format!("/{}", name) } else { format!("{}/{}", parent, name) };
        if self.entries.contains_key(&resolved) { return Err(Error::Exists(resolved)); }
        self.entries.insert(resolved, Entry::Link(target.to_owned()));
        Ok(())
    }

    pub fn read_link(&self, path: &str) -> Result<String, Error> {
        let canonical = self.normalize(path)?;
        if canonical == "/" { return Err(Error::InvalidTarget(0)); }
        let (parent, name) = self.resolve_parent(&canonical)?;
        let resolved = if parent == "/" { format!("/{}", name) } else { format!("{}/{}", parent, name) };
        match self.entries.get(&resolved) {
            Some(Entry::Link(target)) => Ok(target.clone()),
            Some(_) => Err(Error::InvalidTarget(0)),
            None => Err(Error::Missing(resolved)),
        }
    }

    pub fn du(&self, path: &str) -> Result<Vec<(String, u64)>, Error> {
        let canonical = self.normalize(path)?;
        let resolved = self.resolve(&canonical, false)?;
        let root_entry = self.entries.get(&resolved).ok_or_else(|| Error::Missing(resolved.clone()))?;
        let paths: Vec<String> = if matches!(root_entry, Entry::Dir) {
            let mut v = vec![resolved.clone()];
            v.extend(self.entries.keys().filter(|p| *p != &resolved && is_within(p, &resolved)).cloned());
            v
        } else {
            vec![resolved.clone()]
        };
        let mut rows = Vec::new();
        for p in paths {
            let size = match self.entries.get(&p) {
                Some(Entry::File(bytes)) => bytes.len() as u64,
                Some(Entry::Link(target)) => target.len() as u64,
                Some(Entry::Dir) => self.entries.iter()
                    .filter(|(q, _)| *q != &p && is_within(q, &p))
                    .map(|(_, e)| match e {
                        Entry::File(bytes) => bytes.len() as u64,
                        Entry::Link(target) => target.len() as u64,
                        Entry::Dir => 0,
                    })
                    .sum(),
                None => 0,
            };
            rows.push((p, size));
        }
        rows.sort_by(|a, b| a.0.cmp(&b.0));
        Ok(rows)
    }
}

impl Default for Fs {
    fn default() -> Self { Self::new() }
}
