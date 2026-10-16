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
        if path == "/" {
            Vec::new()
        } else {
            path[1..].split('/').map(str::to_owned).collect()
        }
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
            match part {
                "" | "." => {}
                ".." => {
                    parts.pop();
                }
                _ => parts.push(part.to_string()),
            }
        }
        Self::canonical(&parts)
    }

    fn parent(path: &str) -> String {
        if path == "/" {
            return "/".to_string();
        }
        let i = path.rfind('/').unwrap();
        if i == 0 {
            "/".to_string()
        } else {
            path[..i].to_string()
        }
    }

    fn basename(path: &str) -> String {
        if path == "/" {
            String::new()
        } else {
            path.rsplit('/').next().unwrap().to_string()
        }
    }

    fn join(parent: &str, name: &str) -> String {
        if parent == "/" {
            format!("/{name}")
        } else {
            format!("{parent}/{name}")
        }
    }

    fn contains(parent: &str, child: &str) -> bool {
        parent == child
            || (parent == "/" && child.starts_with('/'))
            || (child.starts_with(parent)
                && child.as_bytes().get(parent.len()) == Some(&b'/'))
    }

    fn resolve(
        &self,
        path: &str,
        cwd: &str,
        follow_final: bool,
        create_final: bool,
    ) -> Result<String, Error> {
        let initial = Self::normalize(path, cwd);
        self.resolve_canonical(initial, follow_final, create_final)
    }

    fn resolve_canonical(
        &self,
        mut path: String,
        follow_final: bool,
        create_final: bool,
    ) -> Result<String, Error> {
        let mut visited = BTreeSet::new();
        let mut expansions = 0usize;

        loop {
            let parts = Self::components(&path);
            let mut prefix = "/".to_string();
            let mut restarted = false;

            for (i, component) in parts.iter().enumerate() {
                let next = Self::join(&prefix, component);
                let is_final = i + 1 == parts.len();
                let entry = self.entries.get(&next);

                if entry.is_none() {
                    if create_final && is_final {
                        return Ok(next);
                    }
                    return Err(Error::Missing(next));
                }

                if !is_final {
                    if let Some(Entry::Link(target)) = entry {
                        if !visited.insert(next.clone()) {
                            return Err(Error::Loop(next));
                        }
                        if expansions == 8 {
                            return Err(Error::Depth(next));
                        }
                        expansions += 1;

                        let target_path = Self::normalize(target, &prefix);
                        let suffix = &parts[i + 1..];
                        let mut expanded = Self::components(&target_path);
                        expanded.extend(suffix.iter().cloned());
                        path = Self::canonical(&expanded);
                        restarted = true;
                        break;
                    }
                    if !matches!(entry, Some(Entry::Dir)) {
                        return Err(Error::NotDir(next));
                    }
                } else if follow_final {
                    if let Some(Entry::Link(target)) = entry {
                        if !visited.insert(next.clone()) {
                            return Err(Error::Loop(next));
                        }
                        if expansions == 8 {
                            return Err(Error::Depth(next));
                        }
                        expansions += 1;

                        path = Self::normalize(target, &prefix);
                        restarted = true;
                        break;
                    }
                }

                prefix = next;
            }

            if restarted {
                continue;
            }
            return Ok(path);
        }
    }

    fn entry_kind(entry: &Entry) -> Kind {
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
                candidate.as_str() != path
                    && Self::parent(candidate) == path
                    && candidate.as_str() != "/"
            })
            .cloned()
            .collect()
    }

    pub fn mkdir(&mut self, path: &str) -> Result<(), Error> {
        Self::validate(path)?;
        let canonical = Self::normalize(path, &self.cwd);
        let parent = Self::parent(&canonical);
        let resolved_parent = self.resolve_canonical(parent.clone(), true, false)?;
        if !matches!(self.entries.get(&resolved_parent), Some(Entry::Dir)) {
            return Err(Error::NotDir(resolved_parent));
        }
        let destination = Self::join(&resolved_parent, &Self::basename(&canonical));
        if self.entries.contains_key(&destination) {
            return Err(Error::Exists(destination));
        }
        self.entries.insert(destination, Entry::Dir);
        Ok(())
    }

    pub fn write(&mut self, path: &str, data: &[u8]) -> Result<(), Error> {
        Self::validate(path)?;
        let resolved = self.resolve(path, &self.cwd, true, true)?;
        match self.entries.get_mut(&resolved) {
            Some(Entry::Dir) => Err(Error::IsDir(resolved)),
            Some(Entry::File(contents)) => {
                contents.clear();
                contents.extend_from_slice(data);
                Ok(())
            }
            Some(Entry::Link(_)) => unreachable!(),
            None => {
                let parent = Self::parent(&resolved);
                let resolved_parent = self.resolve_canonical(parent.clone(), true, false)?;
                if !matches!(self.entries.get(&resolved_parent), Some(Entry::Dir)) {
                    return Err(Error::NotDir(resolved_parent));
                }
                let actual = Self::join(&resolved_parent, &Self::basename(&resolved));
                self.entries
                    .insert(actual, Entry::File(data.to_vec()));
                Ok(())
            }
        }
    }

    pub fn read(&self, path: &str) -> Result<Vec<u8>, Error> {
        Self::validate(path)?;
        let resolved = self.resolve(path, &self.cwd, true, false)?;
        match self.entries.get(&resolved) {
            Some(Entry::File(contents)) => Ok(contents.clone()),
            Some(Entry::Dir) => Err(Error::IsDir(resolved)),
            Some(Entry::Link(_)) => unreachable!(),
            None => Err(Error::Missing(resolved)),
        }
    }

    pub fn ls(&self, path: &str) -> Result<Vec<(String, Kind)>, Error> {
        Self::validate(path)?;
        let resolved = self.resolve(path, &self.cwd, true, false)?;
        if !matches!(self.entries.get(&resolved), Some(Entry::Dir)) {
            return Err(Error::NotDir(resolved));
        }
        let mut result: Vec<_> = self
            .children(&resolved)
            .into_iter()
            .map(|child| {
                (
                    Self::basename(&child),
                    Self::entry_kind(self.entries.get(&child).unwrap()),
                )
            })
            .collect();
        result.sort_by(|a, b| a.0.cmp(&b.0));
        Ok(result)
    }

    pub fn cwd(&self) -> String {
        self.cwd.clone()
    }

    pub fn chdir(&mut self, path: &str) -> Result<(), Error> {
        Self::validate(path)?;
        let resolved = self.resolve(path, &self.cwd, true, false)?;
        if !matches!(self.entries.get(&resolved), Some(Entry::Dir)) {
            return Err(Error::NotDir(resolved));
        }
        self.cwd = resolved;
        Ok(())
    }

    pub fn rename(&mut self, from: &str, to: &str, overwrite: bool) -> Result<(), Error> {
        Self::validate(from)?;
        Self::validate(to)?;
        let source = Self::normalize(from, &self.cwd);
        let destination = Self::normalize(to, &self.cwd);
        if source == "/" || destination == "/" {
            return Err(Error::Root);
        }

        let source_parent = Self::parent(&source);
        let source_parent_resolved =
            self.resolve_canonical(source_parent, true, false)?;
        if !matches!(
            self.entries.get(&source_parent_resolved),
            Some(Entry::Dir)
        ) {
            return Err(Error::NotDir(source_parent_resolved));
        }
        let source_path = Self::join(
            &source_parent_resolved,
            &Self::basename(&source),
        );
        let source_entry = self
            .entries
            .get(&source_path)
            .cloned()
            .ok_or_else(|| Error::Missing(source_path.clone()))?;

        if matches!(source_entry, Entry::Dir)
            && Self::contains(&source_path, &self.cwd)
        {
            return Err(Error::Busy(source_path));
        }

        let destination_parent = Self::parent(&destination);
        let destination_parent_resolved =
            self.resolve_canonical(destination_parent, true, false)?;
        if !matches!(
            self.entries.get(&destination_parent_resolved),
            Some(Entry::Dir)
        ) {
            return Err(Error::NotDir(destination_parent_resolved));
        }
        let destination_path = Self::join(
            &destination_parent_resolved,
            &Self::basename(&destination),
        );

        if matches!(self.entries.get(&destination_path), Some(Entry::Dir))
            && Self::contains(&destination_path, &self.cwd)
        {
            return Err(Error::Busy(destination_path));
        }
        if source_path == destination_path {
            return Ok(());
        }
        if matches!(source_entry, Entry::Dir)
            && Self::contains(&source_path, &destination_path)
        {
            return Err(Error::Cycle(destination_path));
        }

        let destination_entry = self.entries.get(&destination_path).cloned();
        if destination_entry.is_some() && !overwrite {
            return Err(Error::Exists(destination_path));
        }
        if let Some(ref target_entry) = destination_entry {
            match (&source_entry, target_entry) {
                (Entry::Dir, Entry::Dir) => {
                    if !self.children(&destination_path).is_empty() {
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
            .filter(|(p, _)| Self::contains(&source_path, p))
            .map(|(p, e)| (p.clone(), e.clone()))
            .collect();

        if let Some(target) = destination_entry {
            if matches!(target, Entry::Dir) {
                self.entries.remove(&destination_path);
            } else {
                self.entries.remove(&destination_path);
            }
        }

        for (old, _) in &moving {
            self.entries.remove(old);
        }
        for (old, entry) in moving {
            let suffix = &old[source_path.len()..];
            self.entries
                .insert(format!("{destination_path}{suffix}"), entry);
        }
        Ok(())
    }

    pub fn remove(&mut self, path: &str, recursive: bool) -> Result<(), Error> {
        Self::validate(path)?;
        let canonical = Self::normalize(path, &self.cwd);
        if canonical == "/" {
            return Err(Error::Root);
        }
        let parent = Self::parent(&canonical);
        let resolved_parent = self.resolve_canonical(parent, true, false)?;
        if !matches!(self.entries.get(&resolved_parent), Some(Entry::Dir)) {
            return Err(Error::NotDir(resolved_parent));
        }
        let target = Self::join(&resolved_parent, &Self::basename(&canonical));
        let entry = self
            .entries
            .get(&target)
            .ok_or_else(|| Error::Missing(target.clone()))?;

        if matches!(entry, Entry::Dir) && Self::contains(&target, &self.cwd) {
            return Err(Error::Busy(target));
        }
        let descendants = self.children(&target);
        if matches!(entry, Entry::Dir) && !recursive && !descendants.is_empty() {
            return Err(Error::NotEmpty(target));
        }

        let remove_paths: Vec<String> = self
            .entries
            .keys()
            .filter(|p| Self::contains(&target, p))
            .cloned()
            .collect();
        for p in remove_paths {
            self.entries.remove(&p);
        }
        Ok(())
    }

    pub fn symlink(&mut self, target: &str, link: &str) -> Result<(), Error> {
        Self::validate(link)?;
        let link_path = Self::normalize(link, &self.cwd);
        if target.is_empty() {
            return Err(Error::InvalidTarget(0));
        }
        if let Some(i) = target.as_bytes().iter().position(|&b| b == 0) {
            return Err(Error::InvalidTarget(i));
        }

        let parent = Self::parent(&link_path);
        let resolved_parent = self.resolve_canonical(parent, true, false)?;
        if !matches!(self.entries.get(&resolved_parent), Some(Entry::Dir)) {
            return Err(Error::NotDir(resolved_parent));
        }
        let actual = Self::join(&resolved_parent, &Self::basename(&link_path));
        if self.entries.contains_key(&actual) {
            return Err(Error::Exists(actual));
        }
        self.entries
            .insert(actual, Entry::Link(target.to_string()));
        Ok(())
    }

    pub fn read_link(&self, path: &str) -> Result<String, Error> {
        Self::validate(path)?;
        let canonical = Self::normalize(path, &self.cwd);
        let parent = Self::parent(&canonical);
        let resolved_parent = self.resolve_canonical(parent, true, false)?;
        if !matches!(self.entries.get(&resolved_parent), Some(Entry::Dir)) {
            return Err(Error::NotDir(resolved_parent));
        }
        let final_path = Self::join(&resolved_parent, &Self::basename(&canonical));
        match self.entries.get(&final_path) {
            Some(Entry::Link(target)) => Ok(target.clone()),
            Some(_) => Err(Error::InvalidTarget(0)),
            None => Err(Error::Missing(final_path)),
        }
    }

    pub fn du(&self, path: &str) -> Result<Vec<(String, u64)>, Error> {
        Self::validate(path)?;
        let resolved = self.resolve(path, &self.cwd, true, false)?;
        let mut rows = Vec::new();
        let mut directory_totals = BTreeMap::<String, u64>::new();

        for (entry_path, entry) in self.entries.iter() {
            if !Self::contains(&resolved, entry_path) {
                continue;
            }
            let size = match entry {
                Entry::File(bytes) => bytes.len() as u64,
                Entry::Link(target) => target.len() as u64,
                Entry::Dir => 0,
            };
            rows.push((entry_path.clone(), size));
            if size != 0 {
                let mut current = Self::parent(entry_path);
                while Self::contains(&resolved, &current) {
                    *directory_totals.entry(current.clone()).or_default() += size;
                    if current == resolved {
                        break;
                    }
                    current = Self::parent(&current);
                }
            }
        }

        for (entry_path, entry) in self.entries.iter() {
            if matches!(entry, Entry::Dir) && Self::contains(&resolved, entry_path) {
                let total = directory_totals.get(entry_path).copied().unwrap_or(0);
                if let Some(row) = rows.iter_mut().find(|row| row.0 == *entry_path) {
                    row.1 = total;
                }
            }
        }
        rows.sort_by(|a, b| a.0.cmp(&b.0));
        Ok(rows)
    }
}