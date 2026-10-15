use std::collections::BTreeMap;

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
        let physical_parent = self.resolve(&parent)?;
        self.require_dir(&physical_parent)?;
        let destination = join(&physical_parent, &name);
        if self.entries.contains_key(&destination) {
            return Err(Error::Exists(destination));
        }
        self.entries.insert(destination, Entry::Dir);
        Ok(())
    }

    pub fn write(&mut self, path: &str, data: &[u8]) -> Result<(), Error> {
        let canonical = normalize(path, &self.cwd)?;
        let resolved = self.resolve_write(&canonical)?;
        match self.entries.get(&resolved) {
            Some(Entry::Dir) => return Err(Error::IsDir(resolved)),
            Some(Entry::File(_)) | None => {}
            Some(Entry::Link(_)) => unreachable!(),
        }
        self.entries.insert(resolved, Entry::File(data.to_vec()));
        Ok(())
    }

    pub fn read(&self, path: &str) -> Result<Vec<u8>, Error> {
        let canonical = normalize(path, &self.cwd)?;
        let resolved = self.resolve(&canonical)?;
        match self.entries.get(&resolved) {
            Some(Entry::File(data)) => Ok(data.clone()),
            Some(Entry::Dir) => Err(Error::IsDir(resolved)),
            Some(Entry::Link(_)) | None => Err(Error::Missing(resolved)),
        }
    }

    pub fn ls(&self, path: &str) -> Result<Vec<(String, Kind)>, Error> {
        let canonical = normalize(path, &self.cwd)?;
        let resolved = self.resolve(&canonical)?;
        self.require_dir(&resolved)?;
        let prefix = if resolved == "/" {
            "/".to_string()
        } else {
            format!("{}/", resolved)
        };
        let mut result = Vec::new();
        for (stored, entry) in &self.entries {
            if stored == &resolved || !stored.starts_with(&prefix) {
                continue;
            }
            let rest = &stored[prefix.len()..];
            if !rest.is_empty() && !rest.contains('/') {
                let kind = match entry {
                    Entry::Dir => Kind::Dir,
                    Entry::File(_) => Kind::File,
                    Entry::Link(_) => Kind::Link,
                };
                result.push((rest.to_string(), kind));
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
        let resolved = self.resolve(&canonical)?;
        self.require_dir(&resolved)?;
        self.cwd = resolved;
        Ok(())
    }

    pub fn rename(&mut self, from: &str, to: &str, overwrite: bool) -> Result<(), Error> {
        let source = normalize(from, &self.cwd)?;
        let destination = normalize(to, &self.cwd)?;
        if source == "/" || destination == "/" {
            return Err(Error::Root);
        }

        let (source_parent, source_name) = split_parent(&source);
        let physical_source_parent = self.resolve(&source_parent)?;
        self.require_dir(&physical_source_parent)?;
        let source_path = join(&physical_source_parent, &source_name);
        let source_entry = self
            .entries
            .get(&source_path)
            .cloned()
            .ok_or_else(|| Error::Missing(source_path.clone()))?;

        if matches!(source_entry, Entry::Dir) && contains(&source_path, &self.cwd) {
            return Err(Error::Busy(source_path));
        }

        let (destination_parent, destination_name) = split_parent(&destination);
        let physical_destination_parent = self.resolve(&destination_parent)?;
        self.require_dir(&physical_destination_parent)?;
        let destination_path = join(&physical_destination_parent, &destination_name);

        if matches!(self.entries.get(&destination_path), Some(Entry::Dir))
            && contains(&destination_path, &self.cwd)
        {
            return Err(Error::Busy(destination_path));
        }

        if source_path == destination_path {
            return Ok(());
        }

        if matches!(source_entry, Entry::Dir) && contains(&source_path, &destination_path) {
            return Err(Error::Cycle(destination_path));
        }

        let destination_entry = self.entries.get(&destination_path).cloned();
        if destination_entry.is_some() && !overwrite {
            return Err(Error::Exists(destination_path));
        }

        if let Some(ref dest_entry) = destination_entry {
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

        let source_prefix = format!("{}/", source_path);
        let moving: Vec<(String, Entry)> = self
            .entries
            .iter()
            .filter(|(p, _)| *p == &source_path || p.starts_with(&source_prefix))
            .map(|(p, e)| (p.clone(), e.clone()))
            .collect();

        if destination_entry.is_some() {
            self.entries.remove(&destination_path);
        }
        for (old_path, _) in &moving {
            self.entries.remove(old_path);
        }
        for (old_path, entry) in moving {
            let suffix = &old_path[source_path.len()..];
            self.entries
                .insert(format!("{}{}", destination_path, suffix), entry);
        }
        Ok(())
    }

    pub fn remove(&mut self, path: &str, recursive: bool) -> Result<(), Error> {
        let canonical = normalize(path, &self.cwd)?;
        if canonical == "/" {
            return Err(Error::Root);
        }
        let (parent, name) = split_parent(&canonical);
        let physical_parent = self.resolve(&parent)?;
        self.require_dir(&physical_parent)?;
        let target = join(&physical_parent, &name);
        let entry = self
            .entries
            .get(&target)
            .cloned()
            .ok_or_else(|| Error::Missing(target.clone()))?;

        if matches!(entry, Entry::Dir) && contains(&target, &self.cwd) {
            return Err(Error::Busy(target));
        }

        if matches!(entry, Entry::Dir) && self.has_children(&target) && !recursive {
            return Err(Error::NotEmpty(target));
        }

        let prefix = format!("{}/", target);
        let removals: Vec<String> = self
            .entries
            .keys()
            .filter(|p| *p == &target || (recursive && p.starts_with(&prefix)))
            .cloned()
            .collect();
        for p in removals {
            self.entries.remove(&p);
        }
        Ok(())
    }

    pub fn symlink(&mut self, target: &str, link: &str) -> Result<(), Error> {
        let canonical_link = normalize(link, &self.cwd)?;
        validate_target(target)?;
        let (parent, name) = split_parent(&canonical_link);
        let physical_parent = self.resolve(&parent)?;
        self.require_dir(&physical_parent)?;
        let link_path = join(&physical_parent, &name);
        if self.entries.contains_key(&link_path) {
            return Err(Error::Exists(link_path));
        }
        self.entries
            .insert(link_path, Entry::Link(target.to_string()));
        Ok(())
    }

    pub fn read_link(&self, path: &str) -> Result<String, Error> {
        let canonical = normalize(path, &self.cwd)?;
        let (parent, name) = split_parent(&canonical);
        let physical_parent = self.resolve(&parent)?;
        self.require_dir(&physical_parent)?;
        let final_path = join(&physical_parent, &name);
        match self.entries.get(&final_path) {
            Some(Entry::Link(target)) => Ok(target.clone()),
            Some(Entry::Dir) | Some(Entry::File(_)) => Err(Error::InvalidTarget(0)),
            None => Err(Error::Missing(final_path)),
        }
    }

    pub fn du(&self, path: &str) -> Result<Vec<(String, u64)>, Error> {
        let canonical = normalize(path, &self.cwd)?;
        let resolved = self.resolve(&canonical)?;
        let prefix = if resolved == "/" {
            "/".to_string()
        } else {
            format!("{}/", resolved)
        };
        let mut paths: Vec<(String, u64)> = self
            .entries
            .iter()
            .filter(|(p, _)| *p == &resolved || p.starts_with(&prefix))
            .map(|(p, entry)| (p.clone(), entry_size(entry)))
            .collect();

        let mut totals: BTreeMap<String, u64> = BTreeMap::new();
        for (p, size) in &paths {
            if !matches!(self.entries.get(p), Some(Entry::Dir)) {
                let mut parent = parent_path(p);
                while contains(&resolved, &parent) {
                    *totals.entry(parent.clone()).or_insert(0) += *size;
                    if parent == resolved {
                        break;
                    }
                    parent = parent_path(&parent);
                }
            }
        }

        for (p, size) in &mut paths {
            if matches!(self.entries.get(p), Some(Entry::Dir)) {
                *size = totals.get(p).copied().unwrap_or(0);
            }
        }
        paths.sort_by(|a, b| a.0.cmp(&b.0));
        Ok(paths)
    }

    fn require_dir(&self, path: &str) -> Result<(), Error> {
        match self.entries.get(path) {
            Some(Entry::Dir) => Ok(()),
            Some(Entry::File(_)) | Some(Entry::Link(_)) => {
                Err(Error::NotDir(path.to_string()))
            }
            None => Err(Error::Missing(path.to_string())),
        }
    }

    fn has_children(&self, path: &str) -> bool {
        let prefix = format!("{}/", path);
        self.entries.keys().any(|p| p.starts_with(&prefix))
    }

    fn resolve(&self, path: &str) -> Result<String, Error> {
        self.resolve_inner(path, false)
    }

    fn resolve_write(&self, path: &str) -> Result<String, Error> {
        self.resolve_inner(path, true)
    }

    fn resolve_inner(&self, path: &str, allow_missing_final: bool) -> Result<String, Error> {
        let mut pending = path_components(path);
        let mut visited = std::collections::BTreeSet::new();
        let mut expansions = 0usize;

        loop {
            let mut current = String::from("/");
            let mut restarted = false;

            for (index, component) in pending.iter().enumerate() {
                let next = join(&current, component);
                let final_component = index + 1 == pending.len();
                match self.entries.get(&next) {
                    None => {
                        if allow_missing_final && final_component {
                            return Ok(next);
                        }
                        return Err(Error::Missing(next));
                    }
                    Some(Entry::Dir) => current = next,
                    Some(Entry::File(_)) => {
                        if !final_component {
                            return Err(Error::NotDir(next));
                        }
                        current = next;
                    }
                    Some(Entry::Link(target)) => {
                        if !visited.insert(next.clone()) {
                            return Err(Error::Loop(next));
                        }
                        if expansions >= 8 {
                            return Err(Error::Depth(next));
                        }
                        expansions += 1;

                        let link_parent = parent_path(&next);
                        let mut expanded = if target.starts_with('/') {
                            Vec::new()
                        } else {
                            path_components(&link_parent)
                        };
                        append_normalized(&mut expanded, target);
                        for suffix in &pending[index + 1..] {
                            append_component(&mut expanded, suffix);
                        }
                        pending = expanded;
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
}

fn validate_path(path: &str) -> Result<(), Error> {
    if let Some(index) = path.as_bytes().iter().position(|b| *b == 0) {
        return Err(Error::InvalidPath(index));
    }
    if path.is_empty() {
        return Err(Error::InvalidPath(0));
    }
    Ok(())
}

fn validate_target(target: &str) -> Result<(), Error> {
    if target.is_empty() {
        return Err(Error::InvalidTarget(0));
    }
    if let Some(index) = target.as_bytes().iter().position(|b| *b == 0) {
        return Err(Error::InvalidTarget(index));
    }
    Ok(())
}

fn normalize(path: &str, cwd: &str) -> Result<String, Error> {
    validate_path(path)?;
    let mut parts = if path.starts_with('/') {
        Vec::new()
    } else {
        path_components(cwd)
    };
    append_normalized(&mut parts, path);
    Ok(path_from_components(&parts))
}

fn path_components(path: &str) -> Vec<String> {
    let mut parts = Vec::new();
    append_normalized(&mut parts, path);
    parts
}

fn append_normalized(parts: &mut Vec<String>, path: &str) {
    for component in path.split('/') {
        append_component(parts, component);
    }
}

fn append_component(parts: &mut Vec<String>, component: &str) {
    match component {
        "" | "." => {}
        ".." => {
            parts.pop();
        }
        _ => parts.push(component.to_string()),
    }
}

fn path_from_components(parts: &[String]) -> String {
    if parts.is_empty() {
        "/".to_string()
    } else {
        format!("/{}", parts.join("/"))
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

fn parent_path(path: &str) -> String {
    split_parent(path).0
}

fn join(parent: &str, name: &str) -> String {
    if parent == "/" {
        format!("/{}", name)
    } else {
        format!("{}/{}", parent, name)
    }
}

fn contains(parent: &str, child: &str) -> bool {
    parent == child
        || parent == "/"
        || child
            .strip_prefix(parent)
            .map(|tail| tail.starts_with('/'))
            .unwrap_or(false)
}

fn entry_size(entry: &Entry) -> u64 {
    match entry {
        Entry::Dir => 0,
        Entry::File(data) => data.len() as u64,
        Entry::Link(target) => target.len() as u64,
    }
}