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

fn normalize_components<I, S>(components: I) -> Vec<String>
where
    I: IntoIterator<Item = S>,
    S: AsRef<str>,
{
    let mut result = Vec::new();
    for component in components {
        let c = component.as_ref();
        match c {
            "" | "." => {}
            ".." => {
                result.pop();
            }
            _ => result.push(c.to_string()),
        }
    }
    result
}

fn components(path: &str) -> Vec<String> {
    path.split('/')
        .filter(|c| !c.is_empty())
        .map(str::to_string)
        .collect()
}

fn canonical(parts: &[String]) -> String {
    if parts.is_empty() {
        "/".to_string()
    } else {
        format!("/{}", parts.join("/"))
    }
}

fn parent_path(path: &str) -> String {
    match path.rfind('/') {
        Some(0) | None => "/".to_string(),
        Some(i) => path[..i].to_string(),
    }
}

fn basename(path: &str) -> String {
    path.rsplit('/').next().unwrap_or("").to_string()
}

fn is_within(path: &str, root: &str) -> bool {
    if root == "/" {
        return path.starts_with('/');
    }
    path == root
        || path
            .strip_prefix(root)
            .map(|s| s.starts_with('/'))
            .unwrap_or(false)
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
        validate(path)?;
        let mut parts = if path.starts_with('/') {
            Vec::new()
        } else {
            components(&self.cwd)
        };
        parts = normalize_components(parts.into_iter().chain(components(path)));
        Ok(canonical(&parts))
    }

    fn resolve(
        &self,
        path: &str,
        follow_final: bool,
        create_final: bool,
    ) -> Result<String, Error> {
        let mut parts = components(path);
        let mut visited = BTreeSet::new();
        let mut expansions = 0usize;

        loop {
            let mut physical: Vec<String> = Vec::new();
            let mut restart = None;

            for (index, component) in parts.iter().enumerate() {
                physical.push(component.clone());
                let current = canonical(&physical);
                let is_final = index + 1 == parts.len();

                let entry = match self.entries.get(&current) {
                    Some(entry) => entry,
                    None => {
                        if create_final && is_final {
                            let parent = canonical(&physical[..physical.len() - 1]);
                            match self.entries.get(&parent) {
                                Some(Entry::Dir) => return Ok(current),
                                Some(_) => return Err(Error::NotDir(parent)),
                                None => return Err(Error::Missing(parent)),
                            }
                        }
                        return Err(Error::Missing(current));
                    }
                };

                if !is_final {
                    match entry {
                        Entry::Dir => {}
                        Entry::Link(target) => {
                            if !visited.insert(current.clone()) {
                                return Err(Error::Loop(current));
                            }
                            if expansions >= 8 {
                                return Err(Error::Depth(current));
                            }
                            expansions += 1;
                            let base = if target.starts_with('/') {
                                Vec::new()
                            } else {
                                physical[..physical.len() - 1].to_vec()
                            };
                            let target_parts =
                                normalize_components(base.into_iter().chain(components(target)));
                            let combined = normalize_components(
                                target_parts.into_iter().chain(parts[index + 1..].iter().cloned()),
                            );
                            restart = Some(combined);
                            break;
                        }
                        Entry::File(_) => return Err(Error::NotDir(current)),
                    }
                } else if follow_final {
                    if let Entry::Link(target) = entry {
                        if !visited.insert(current.clone()) {
                            return Err(Error::Loop(current));
                        }
                        if expansions >= 8 {
                            return Err(Error::Depth(current));
                        }
                        expansions += 1;
                        let base = if target.starts_with('/') {
                            Vec::new()
                        } else {
                            physical[..physical.len() - 1].to_vec()
                        };
                        let target_parts =
                            normalize_components(base.into_iter().chain(components(target)));
                        restart = Some(target_parts);
                        break;
                    }
                }
            }

            if let Some(new_parts) = restart {
                parts = new_parts;
                continue;
            }

            return Ok(canonical(&parts));
        }
    }

    fn resolve_parent(&self, path: &str) -> Result<String, Error> {
        let parent = parent_path(path);
        self.resolve(&parent, true, false)
    }

    fn kind(entry: &Entry) -> Kind {
        match entry {
            Entry::Dir => Kind::Dir,
            Entry::File(_) => Kind::File,
            Entry::Link(_) => Kind::Link,
        }
    }

    fn children(&self, path: &str) -> Vec<String> {
        self.entries
            .keys()
            .filter(|candidate| {
                *candidate != path && parent_path(candidate) == path
            })
            .cloned()
            .collect()
    }

    pub fn mkdir(&mut self, path: &str) -> Result<(), Error> {
        let normalized = self.normalize(path)?;
        let parent = self.resolve_parent(&normalized)?;
        if !matches!(self.entries.get(&parent), Some(Entry::Dir)) {
            return Err(Error::NotDir(parent));
        }

        let name = basename(&normalized);
        let resolved = if normalized == "/" {
            "/".to_string()
        } else {
            canonical(
                &components(&parent)
                    .into_iter()
                    .chain(std::iter::once(name))
                    .collect::<Vec<_>>(),
            )
        };

        if self.entries.contains_key(&resolved) {
            return Err(Error::Exists(resolved));
        }
        self.entries.insert(resolved, Entry::Dir);
        Ok(())
    }

    pub fn write(&mut self, path: &str, data: &[u8]) -> Result<(), Error> {
        let normalized = self.normalize(path)?;
        let resolved = self.resolve(&normalized, true, true)?;
        match self.entries.get(&resolved) {
            Some(Entry::Dir) => Err(Error::IsDir(resolved)),
            Some(Entry::File(_)) => {
                self.entries
                    .insert(resolved, Entry::File(data.to_vec()));
                Ok(())
            }
            Some(Entry::Link(_)) => unreachable!(),
            None => {
                let parent = parent_path(&resolved);
                if !matches!(self.entries.get(&parent), Some(Entry::Dir)) {
                    return Err(Error::NotDir(parent));
                }
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
            _ => Err(Error::Missing(resolved)),
        }
    }

    pub fn ls(&self, path: &str) -> Result<Vec<(String, Kind)>, Error> {
        let normalized = self.normalize(path)?;
        let resolved = self.resolve(&normalized, true, false)?;
        if !matches!(self.entries.get(&resolved), Some(Entry::Dir)) {
            return Err(Error::NotDir(resolved));
        }

        let mut result: Vec<_> = self
            .children(&resolved)
            .into_iter()
            .filter_map(|child| {
                self.entries
                    .get(&child)
                    .map(|entry| (basename(&child), Self::kind(entry)))
            })
            .collect();
        result.sort_by(|a, b| a.0.cmp(&b.0));
        Ok(result)
    }

    pub fn cwd(&self) -> String {
        self.cwd.clone()
    }

    pub fn chdir(&mut self, path: &str) -> Result<(), Error> {
        let normalized = self.normalize(path)?;
        let resolved = self.resolve(&normalized, true, false)?;
        if !matches!(self.entries.get(&resolved), Some(Entry::Dir)) {
            return Err(Error::NotDir(resolved));
        }
        self.cwd = resolved;
        Ok(())
    }

    pub fn rename(&mut self, from: &str, to: &str, overwrite: bool) -> Result<(), Error> {
        let source = self.normalize(from)?;
        let destination = self.normalize(to)?;

        if source == "/" || destination == "/" {
            return Err(Error::Root);
        }

        let source_parent = self.resolve_parent(&source)?;
        if !matches!(self.entries.get(&source_parent), Some(Entry::Dir)) {
            return Err(Error::NotDir(source_parent));
        }

        let source_resolved = canonical(
            &components(&source_parent)
                .into_iter()
                .chain(std::iter::once(basename(&source)))
                .collect::<Vec<_>>(),
        );
        let source_entry = self
            .entries
            .get(&source_resolved)
            .cloned()
            .ok_or_else(|| Error::Missing(source_resolved.clone()))?;

        if matches!(source_entry, Entry::Dir) && is_within(&self.cwd, &source_resolved) {
            return Err(Error::Busy(source_resolved));
        }

        let destination_parent = self.resolve_parent(&destination)?;
        if !matches!(self.entries.get(&destination_parent), Some(Entry::Dir)) {
            return Err(Error::NotDir(destination_parent));
        }
        let destination_resolved = canonical(
            &components(&destination_parent)
                .into_iter()
                .chain(std::iter::once(basename(&destination)))
                .collect::<Vec<_>>(),
        );

        if matches!(self.entries.get(&destination_resolved), Some(Entry::Dir))
            && is_within(&self.cwd, &destination_resolved)
        {
            return Err(Error::Busy(destination_resolved));
        }

        if source_resolved == destination_resolved {
            return Ok(());
        }

        if matches!(source_entry, Entry::Dir) && is_within(&destination_resolved, &source_resolved)
        {
            return Err(Error::Cycle(destination_resolved));
        }

        let destination_entry = self.entries.get(&destination_resolved).cloned();
        if destination_entry.is_some() && !overwrite {
            return Err(Error::Exists(destination_resolved));
        }

        if let Some(dest_entry) = &destination_entry {
            match (&source_entry, dest_entry) {
                (Entry::Dir, Entry::Dir) => {
                    if !self.children(&destination_resolved).is_empty() {
                        return Err(Error::NotEmpty(destination_resolved));
                    }
                }
                (Entry::Dir, _) => return Err(Error::NotDir(destination_resolved)),
                (_, Entry::Dir) => return Err(Error::IsDir(destination_resolved)),
                _ => {}
            }
        }

        let moving: Vec<(String, Entry)> = self
            .entries
            .iter()
            .filter(|(path, _)| is_within(path, &source_resolved))
            .map(|(path, entry)| (path.clone(), entry.clone()))
            .collect();

        if destination_entry.is_some() {
            self.entries.remove(&destination_resolved);
        }
        for (path, _) in &moving {
            self.entries.remove(path);
        }
        for (old_path, entry) in moving {
            let suffix = &old_path[source_resolved.len()..];
            self.entries
                .insert(format!("{destination_resolved}{suffix}"), entry);
        }
        Ok(())
    }

    pub fn remove(&mut self, path: &str, recursive: bool) -> Result<(), Error> {
        let normalized = self.normalize(path)?;
        if normalized == "/" {
            return Err(Error::Root);
        }

        let parent = self.resolve_parent(&normalized)?;
        if !matches!(self.entries.get(&parent), Some(Entry::Dir)) {
            return Err(Error::NotDir(parent));
        }
        let resolved = canonical(
            &components(&parent)
                .into_iter()
                .chain(std::iter::once(basename(&normalized)))
                .collect::<Vec<_>>(),
        );
        let entry = self
            .entries
            .get(&resolved)
            .ok_or_else(|| Error::Missing(resolved.clone()))?;

        if matches!(entry, Entry::Dir) && is_within(&self.cwd, &resolved) {
            return Err(Error::Busy(resolved));
        }

        let children = self.children(&resolved);
        if matches!(entry, Entry::Dir) && !children.is_empty() && !recursive {
            return Err(Error::NotEmpty(resolved));
        }

        if recursive && matches!(entry, Entry::Dir) {
            let to_remove: Vec<_> = self
                .entries
                .keys()
                .filter(|candidate| is_within(candidate, &resolved))
                .cloned()
                .collect();
            for candidate in to_remove {
                self.entries.remove(&candidate);
            }
        } else {
            self.entries.remove(&resolved);
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

        let parent = self.resolve_parent(&normalized_link)?;
        if !matches!(self.entries.get(&parent), Some(Entry::Dir)) {
            return Err(Error::NotDir(parent));
        }

        let resolved = if normalized_link == "/" {
            "/".to_string()
        } else {
            canonical(
                &components(&parent)
                    .into_iter()
                    .chain(std::iter::once(basename(&normalized_link)))
                    .collect::<Vec<_>>(),
            )
        };
        if self.entries.contains_key(&resolved) {
            return Err(Error::Exists(resolved));
        }
        self.entries
            .insert(resolved, Entry::Link(target.to_string()));
        Ok(())
    }

    pub fn read_link(&self, path: &str) -> Result<String, Error> {
        let normalized = self.normalize(path)?;
        let parent = self.resolve_parent(&normalized)?;
        if !matches!(self.entries.get(&parent), Some(Entry::Dir)) {
            return Err(Error::NotDir(parent));
        }
        let resolved = if normalized == "/" {
            "/".to_string()
        } else {
            canonical(
                &components(&parent)
                    .into_iter()
                    .chain(std::iter::once(basename(&normalized)))
                    .collect::<Vec<_>>(),
            )
        };
        match self.entries.get(&resolved) {
            Some(Entry::Link(target)) => Ok(target.clone()),
            Some(_) => Err(Error::InvalidTarget(0)),
            None => Err(Error::Missing(resolved)),
        }
    }

    pub fn du(&self, path: &str) -> Result<Vec<(String, u64)>, Error> {
        let normalized = self.normalize(path)?;
        let resolved = self.resolve(&normalized, true, false)?;
        if !self.entries.contains_key(&resolved) {
            return Err(Error::Missing(resolved));
        }

        let paths: Vec<String> = self
            .entries
            .keys()
            .filter(|candidate| is_within(candidate, &resolved))
            .cloned()
            .collect();

        let mut sizes = BTreeMap::new();
        for item in &paths {
            let size = match self.entries.get(item).unwrap() {
                Entry::Dir => 0,
                Entry::File(data) => data.len() as u64,
                Entry::Link(target) => target.len() as u64,
            };
            sizes.insert(item.clone(), size);
        }

        for item in paths.iter().rev() {
            if item != &resolved {
                let parent = parent_path(item);
                let value = *sizes.get(item).unwrap_or(&0);
                if let Some(parent_size) = sizes.get_mut(&parent) {
                    *parent_size += value;
                }
            }
        }

        let mut result: Vec<_> = paths
            .into_iter()
            .map(|item| {
                let size = match self.entries.get(&item).unwrap() {
                    Entry::Dir => *sizes.get(&item).unwrap_or(&0),
                    Entry::File(data) => data.len() as u64,
                    Entry::Link(target) => target.len() as u64,
                };
                (item, size)
            })
            .collect();
        result.sort_by(|a, b| a.0.cmp(&b.0));
        Ok(result)
    }
}