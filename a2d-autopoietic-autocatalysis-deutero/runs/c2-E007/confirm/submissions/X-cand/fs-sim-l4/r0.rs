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

#[derive(Debug, Clone)]
struct ParsedPath {
    absolute: bool,
    parts: Vec<String>,
}

pub struct Fs {
    entries: std::collections::BTreeMap<String, Entry>,
    cwd: String,
}

impl Default for Fs {
    fn default() -> Self {
        Self::new()
    }
}

impl Fs {
    pub fn new() -> Self {
        let mut entries = std::collections::BTreeMap::new();
        entries.insert("/".to_string(), Entry::Dir);
        Self {
            entries,
            cwd: "/".to_string(),
        }
    }

    fn validate_path(path: &str) -> Result<ParsedPath, Error> {
        if let Some(i) = path.as_bytes().iter().position(|&b| b == 0) {
            return Err(Error::InvalidPath(i));
        }
        if path.is_empty() {
            return Err(Error::InvalidPath(0));
        }
        Ok(ParsedPath {
            absolute: path.starts_with('/'),
            parts: path
                .split('/')
                .filter(|p| !p.is_empty() && *p != ".")
                .map(str::to_string)
                .collect(),
        })
    }

    fn push_normalized(parts: &mut Vec<String>, part: &str) {
        match part {
            "" | "." => {}
            ".." => {
                parts.pop();
            }
            _ => parts.push(part.to_string()),
        }
    }

    fn path_from_parts(parts: &[String]) -> String {
        if parts.is_empty() {
            "/".to_string()
        } else {
            format!("/{}", parts.join("/"))
        }
    }

    fn canonical(&self, parsed: &ParsedPath) -> String {
        let mut parts = if parsed.absolute {
            Vec::new()
        } else if self.cwd == "/" {
            Vec::new()
        } else {
            self.cwd[1..].split('/').map(str::to_string).collect()
        };
        for part in &parsed.parts {
            Self::push_normalized(&mut parts, part);
        }
        Self::path_from_parts(&parts)
    }

    fn components(path: &str) -> Vec<String> {
        if path == "/" {
            Vec::new()
        } else {
            path[1..].split('/').map(str::to_string).collect()
        }
    }

    fn parent(path: &str) -> String {
        if path == "/" {
            return "/".to_string();
        }
        let mut parts = Self::components(path);
        parts.pop();
        Self::path_from_parts(&parts)
    }

    fn basename(path: &str) -> String {
        if path == "/" {
            String::new()
        } else {
            path.rsplit('/').next().unwrap_or("").to_string()
        }
    }

    fn join_child(parent: &str, name: &str) -> String {
        if parent == "/" {
            format!("/{}", name)
        } else {
            format!("{}/{}", parent, name)
        }
    }

    fn is_descendant_or_same(path: &str, base: &str) -> bool {
        if base == "/" {
            return true;
        }
        path == base
            || path
                .strip_prefix(base)
                .map(|rest| rest.starts_with('/'))
                .unwrap_or(false)
    }

    fn direct_children(&self, path: &str) -> Vec<(String, Kind)> {
        let mut out = Vec::new();
        for (candidate, entry) in &self.entries {
            if candidate == path {
                continue;
            }
            if Self::parent(candidate) == path {
                let kind = match entry {
                    Entry::Dir => Kind::Dir,
                    Entry::File(_) => Kind::File,
                    Entry::Link(_) => Kind::Link,
                };
                out.push((Self::basename(candidate), kind));
            }
        }
        out.sort_by(|a, b| a.0.cmp(&b.0));
        out
    }

    fn has_children(&self, path: &str) -> bool {
        self.entries
            .keys()
            .any(|candidate| candidate != path && Self::is_descendant_or_same(candidate, path))
    }

    fn resolve(
        &self,
        path: &str,
        follow_final: bool,
        create_missing_final: bool,
    ) -> Result<String, Error> {
        let mut pending = Self::components(path);
        let mut resolved: Vec<String> = Vec::new();
        let mut visited = std::collections::BTreeSet::<String>::new();
        let mut expansions = 0usize;

        while !pending.is_empty() {
            let component = pending.remove(0);
            let candidate_parts = {
                let mut p = resolved.clone();
                p.push(component.clone());
                p
            };
            let candidate = Self::path_from_parts(&candidate_parts);
            let is_final = pending.is_empty();

            match self.entries.get(&candidate) {
                None => {
                    if is_final && create_missing_final {
                        return Ok(candidate);
                    }
                    return Err(Error::Missing(candidate));
                }
                Some(entry) => {
                    if let Entry::Link(target) = entry {
                        if follow_final || !is_final {
                            if visited.contains(&candidate) {
                                return Err(Error::Loop(candidate));
                            }
                            visited.insert(candidate.clone());
                            if expansions == 8 {
                                return Err(Error::Depth(candidate));
                            }
                            expansions += 1;

                            let parsed_target = Self::validate_path(target)
                                .map_err(|e| match e {
                                    Error::InvalidPath(i) => Error::InvalidTarget(i),
                                    _ => Error::InvalidTarget(0),
                                })?;
                            let mut target_parts = if parsed_target.absolute {
                                Vec::new()
                            } else {
                                resolved.clone()
                            };
                            for part in parsed_target.parts {
                                Self::push_normalized(&mut target_parts, &part);
                            }
                            for suffix in pending.drain(..) {
                                Self::push_normalized(&mut target_parts, &suffix);
                            }
                            pending = target_parts;
                            resolved.clear();
                            continue;
                        }
                    }

                    if !is_final && !matches!(entry, Entry::Dir) {
                        return Err(Error::NotDir(candidate));
                    }
                    resolved.push(component);
                }
            }
        }

        Ok(Self::path_from_parts(&resolved))
    }

    fn resolve_parent(&self, path: &str) -> Result<(String, String), Error> {
        let parent = Self::parent(path);
        let name = Self::basename(path);
        let physical_parent = self.resolve(&parent, true, false)?;
        match self.entries.get(&physical_parent) {
            Some(Entry::Dir) => Ok((physical_parent, name)),
            Some(_) => Err(Error::NotDir(physical_parent)),
            None => Err(Error::Missing(physical_parent)),
        }
    }

    fn final_entry_path(
        &self,
        path: &str,
        follow_final: bool,
        create_missing_final: bool,
    ) -> Result<String, Error> {
        self.resolve(path, follow_final, create_missing_final)
    }

    fn remove_subtree(&mut self, path: &str) {
        let doomed: Vec<String> = self
            .entries
            .keys()
            .filter(|p| Self::is_descendant_or_same(p, path))
            .cloned()
            .collect();
        for p in doomed {
            self.entries.remove(&p);
        }
    }

    fn subtree_size(&self, path: &str) -> u64 {
        self.entries
            .iter()
            .filter(|(p, _)| Self::is_descendant_or_same(p, path))
            .map(|(_, entry)| match entry {
                Entry::Dir => 0,
                Entry::File(data) => data.len() as u64,
                Entry::Link(target) => target.len() as u64,
            })
            .sum()
    }

    pub fn mkdir(&mut self, path: &str) -> Result<(), Error> {
        let parsed = Self::validate_path(path)?;
        let canonical = self.canonical(&parsed);
        if canonical == "/" {
            return Err(Error::Exists("/".to_string()));
        }
        let (parent, name) = self.resolve_parent(&canonical)?;
        let destination = Self::join_child(&parent, &name);
        if self.entries.contains_key(&destination) {
            return Err(Error::Exists(destination));
        }
        self.entries.insert(destination, Entry::Dir);
        Ok(())
    }

    pub fn write(&mut self, path: &str, data: &[u8]) -> Result<(), Error> {
        let parsed = Self::validate_path(path)?;
        let canonical = self.canonical(&parsed);
        let target = self.final_entry_path(&canonical, true, true)?;
        match self.entries.get(&target) {
            Some(Entry::Dir) => Err(Error::IsDir(target)),
            Some(Entry::File(_)) => {
                self.entries.insert(target, Entry::File(data.to_vec()));
                Ok(())
            }
            Some(Entry::Link(_)) => {
                // A final link can only remain if a loop was deliberately not followed.
                Err(Error::Loop(target))
            }
            None => {
                let (parent, _) = self.resolve_parent(&target)?;
                if parent == target {
                    return Err(Error::IsDir(target));
                }
                self.entries.insert(target, Entry::File(data.to_vec()));
                Ok(())
            }
        }
    }

    pub fn read(&self, path: &str) -> Result<Vec<u8>, Error> {
        let parsed = Self::validate_path(path)?;
        let canonical = self.canonical(&parsed);
        let target = self.final_entry_path(&canonical, true, false)?;
        match self.entries.get(&target) {
            Some(Entry::File(data)) => Ok(data.clone()),
            Some(Entry::Dir) => Err(Error::IsDir(target)),
            Some(Entry::Link(_)) => Err(Error::Loop(target)),
            None => Err(Error::Missing(target)),
        }
    }

    pub fn ls(&self, path: &str) -> Result<Vec<(String, Kind)>, Error> {
        let parsed = Self::validate_path(path)?;
        let canonical = self.canonical(&parsed);
        let target = self.final_entry_path(&canonical, true, false)?;
        match self.entries.get(&target) {
            Some(Entry::Dir) => Ok(self.direct_children(&target)),
            Some(_) => Err(Error::NotDir(target)),
            None => Err(Error::Missing(target)),
        }
    }

    pub fn cwd(&self) -> String {
        self.cwd.clone()
    }

    pub fn chdir(&mut self, path: &str) -> Result<(), Error> {
        let parsed = Self::validate_path(path)?;
        let canonical = self.canonical(&parsed);
        let target = self.final_entry_path(&canonical, true, false)?;
        match self.entries.get(&target) {
            Some(Entry::Dir) => {
                self.cwd = target;
                Ok(())
            }
            Some(_) => Err(Error::NotDir(target)),
            None => Err(Error::Missing(target)),
        }
    }

    pub fn rename(&mut self, from: &str, to: &str, overwrite: bool) -> Result<(), Error> {
        let from_parsed = Self::validate_path(from)?;
        let to_parsed = Self::validate_path(to)?;
        let source = self.canonical(&from_parsed);
        let destination_lexical = self.canonical(&to_parsed);

        if source == "/" || destination_lexical == "/" {
            return Err(Error::Root);
        }

        let (source_parent, source_name) = self.resolve_parent(&source)?;
        let source_path = Self::join_child(&source_parent, &source_name);
        let source_entry = self
            .entries
            .get(&source_path)
            .cloned()
            .ok_or_else(|| Error::Missing(source_path.clone()))?;

        if matches!(source_entry, Entry::Dir) && Self::is_descendant_or_same(&self.cwd, &source_path)
        {
            return Err(Error::Busy(source_path));
        }

        let (destination_parent, destination_name) = self.resolve_parent(&destination_lexical)?;
        let destination = Self::join_child(&destination_parent, &destination_name);

        if matches!(self.entries.get(&destination), Some(Entry::Dir))
            && Self::is_descendant_or_same(&self.cwd, &destination)
        {
            return Err(Error::Busy(destination));
        }

        if source_path == destination {
            return Ok(());
        }

        if matches!(source_entry, Entry::Dir)
            && Self::is_descendant_or_same(&destination, &source_path)
        {
            return Err(Error::Cycle(destination));
        }

        let existing = self.entries.get(&destination).cloned();
        if existing.is_some() && !overwrite {
            return Err(Error::Exists(destination));
        }

        if let Some(existing_entry) = &existing {
            let source_is_dir = matches!(source_entry, Entry::Dir);
            let destination_is_dir = matches!(existing_entry, Entry::Dir);
            match (source_is_dir, destination_is_dir) {
                (true, false) => return Err(Error::NotDir(destination)),
                (false, true) => return Err(Error::IsDir(destination)),
                (true, true) if self.has_children(&destination) => {
                    return Err(Error::NotEmpty(destination));
                }
                _ => {}
            }
        }

        let moving: Vec<(String, Entry)> = self
            .entries
            .iter()
            .filter(|(p, _)| Self::is_descendant_or_same(p, &source_path))
            .map(|(p, e)| (p.clone(), e.clone()))
            .collect();

        if existing.is_some() {
            self.remove_subtree(&destination);
        }
        self.remove_subtree(&source_path);

        for (old_path, entry) in moving {
            let suffix = if old_path == source_path {
                String::new()
            } else {
                old_path[source_path.len()..].to_string()
            };
            let new_path = format!("{}{}", destination, suffix);
            self.entries.insert(new_path, entry);
        }
        Ok(())
    }

    pub fn remove(&mut self, path: &str, recursive: bool) -> Result<(), Error> {
        let parsed = Self::validate_path(path)?;
        let canonical = self.canonical(&parsed);
        if canonical == "/" {
            return Err(Error::Root);
        }

        let (parent, name) = self.resolve_parent(&canonical)?;
        let target = Self::join_child(&parent, &name);
        let entry = self
            .entries
            .get(&target)
            .cloned()
            .ok_or_else(|| Error::Missing(target.clone()))?;

        if matches!(entry, Entry::Dir) && Self::is_descendant_or_same(&self.cwd, &target) {
            return Err(Error::Busy(target));
        }
        if matches!(entry, Entry::Dir) && self.has_children(&target) && !recursive {
            return Err(Error::NotEmpty(target));
        }

        self.remove_subtree(&target);
        Ok(())
    }

    pub fn symlink(&mut self, target: &str, link: &str) -> Result<(), Error> {
        let link_parsed = Self::validate_path(link)?;
        let link_canonical = self.canonical(&link_parsed);

        if target.is_empty() {
            return Err(Error::InvalidTarget(0));
        }
        if let Some(i) = target.as_bytes().iter().position(|&b| b == 0) {
            return Err(Error::InvalidTarget(i));
        }

        if link_canonical == "/" {
            return Err(Error::Exists("/".to_string()));
        }
        let (parent, name) = self.resolve_parent(&link_canonical)?;
        let link_path = Self::join_child(&parent, &name);
        if self.entries.contains_key(&link_path) {
            return Err(Error::Exists(link_path));
        }
        self.entries
            .insert(link_path, Entry::Link(target.to_string()));
        Ok(())
    }

    pub fn read_link(&self, path: &str) -> Result<String, Error> {
        let parsed = Self::validate_path(path)?;
        let canonical = self.canonical(&parsed);
        let final_path = self.final_entry_path(&canonical, false, false)?;
        match self.entries.get(&final_path) {
            Some(Entry::Link(target)) => Ok(target.clone()),
            Some(_) => Err(Error::InvalidTarget(0)),
            None => Err(Error::Missing(final_path)),
        }
    }

    pub fn du(&self, path: &str) -> Result<Vec<(String, u64)>, Error> {
        let parsed = Self::validate_path(path)?;
        let canonical = self.canonical(&parsed);
        let target = self.final_entry_path(&canonical, true, false)?;
        if !self.entries.contains_key(&target) {
            return Err(Error::Missing(target));
        }

        let mut paths: Vec<String> = self
            .entries
            .keys()
            .filter(|p| Self::is_descendant_or_same(p, &target))
            .cloned()
            .collect();
        paths.sort();

        let rows = paths
            .into_iter()
            .map(|p| {
                let size = match self.entries.get(&p).expect("entry path was collected") {
                    Entry::Dir => self.subtree_size(&p),
                    Entry::File(data) => data.len() as u64,
                    Entry::Link(link_target) => link_target.len() as u64,
                };
                (p, size)
            })
            .collect();
        Ok(rows)
    }
}