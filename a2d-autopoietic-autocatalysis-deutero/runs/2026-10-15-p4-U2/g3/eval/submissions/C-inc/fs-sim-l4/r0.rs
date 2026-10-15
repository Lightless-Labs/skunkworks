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

#[derive(Debug, Clone)]
enum Entry {
    Dir,
    File(Vec<u8>),
    Link(String),
}

pub struct Fs {
    entries: BTreeMap<String, Entry>,
    cwd: String,
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

    fn path_components(path: &str) -> Result<(), Error> {
        if let Some(i) = path.as_bytes().iter().position(|&b| b == 0) {
            return Err(Error::InvalidPath(i));
        }
        if path.is_empty() {
            return Err(Error::InvalidPath(0));
        }
        Ok(())
    }

    fn normalize(path: &str, base: &str) -> Result<String, Error> {
        Self::path_components(path)?;
        let mut parts: Vec<&str> = Vec::new();
        if !path.starts_with('/') {
            parts.extend(base.split('/').filter(|p| !p.is_empty()));
        }
        for part in path.split('/') {
            match part {
                "" | "." => {}
                ".." => {
                    parts.pop();
                }
                _ => parts.push(part),
            }
        }
        if parts.is_empty() {
            Ok("/".to_string())
        } else {
            Ok(format!("/{}", parts.join("/")))
        }
    }

    fn parent(path: &str) -> (&str, &str) {
        if path == "/" {
            return ("/", "");
        }
        match path.rfind('/') {
            Some(0) => ("/", &path[1..]),
            Some(i) => (&path[..i], &path[i + 1..]),
            None => ("/", path),
        }
    }

    fn join_child(parent: &str, name: &str) -> String {
        if parent == "/" {
            format!("/{}", name)
        } else {
            format!("{}/{}", parent, name)
        }
    }

    fn is_within(path: &str, dir: &str) -> bool {
        if dir == "/" {
            path.starts_with('/')
        } else {
            path == dir
                || path
                    .strip_prefix(dir)
                    .is_some_and(|rest| rest.starts_with('/'))
        }
    }

    fn kind(entry: &Entry) -> Kind {
        match entry {
            Entry::Dir => Kind::Dir,
            Entry::File(_) => Kind::File,
            Entry::Link(_) => Kind::Link,
        }
    }

    fn link_base(link_path: &str) -> &str {
        Self::parent(link_path).0
    }

    fn resolve(
        &self,
        path: &str,
        follow_final: bool,
        create_missing_final: bool,
    ) -> Result<String, Error> {
        let mut current = "/".to_string();
        let mut todo: Vec<String> = if path == "/" {
            Vec::new()
        } else {
            path[1..].split('/').map(str::to_string).collect()
        };
        let mut visited = HashSet::new();
        let mut expansions = 0usize;

        while !todo.is_empty() {
            let component = todo.remove(0);
            let candidate = Self::join_child(&current, &component);
            let is_final = todo.is_empty();

            let Some(entry) = self.entries.get(&candidate) else {
                if is_final && create_missing_final {
                    return Ok(candidate);
                }
                return Err(Error::Missing(candidate));
            };

            match entry {
                Entry::Dir => current = candidate,
                Entry::File(_) => {
                    if is_final {
                        current = candidate;
                    } else {
                        return Err(Error::NotDir(candidate));
                    }
                }
                Entry::Link(target) if follow_final || !is_final => {
                    if !visited.insert(candidate.clone()) {
                        return Err(Error::Loop(candidate));
                    }
                    if expansions >= 8 {
                        return Err(Error::Depth(candidate));
                    }
                    expansions += 1;

                    let target_path = Self::normalize(target, Self::link_base(&candidate))
                        .map_err(|e| match e {
                            Error::InvalidPath(i) => Error::InvalidTarget(i),
                            other => other,
                        })?;
                    let mut expanded: Vec<String> = if target_path == "/" {
                        Vec::new()
                    } else {
                        target_path[1..].split('/').map(str::to_string).collect()
                    };
                    expanded.extend(todo.into_iter());
                    let combined = if expanded.is_empty() {
                        "/".to_string()
                    } else {
                        format!("/{}", expanded.join("/"))
                    };

                    current = "/".to_string();
                    todo = if combined == "/" {
                        Vec::new()
                    } else {
                        combined[1..].split('/').map(str::to_string).collect()
                    };
                }
                Entry::Link(_) => current = candidate,
            }
        }

        Ok(current)
    }

    fn resolve_parent(&self, path: &str) -> Result<(String, String), Error> {
        let (parent, name) = Self::parent(path);
        let resolved_parent = self.resolve(parent, true, false)?;
        match self.entries.get(&resolved_parent) {
            Some(Entry::Dir) => Ok((resolved_parent, name.to_string())),
            Some(_) => Err(Error::NotDir(resolved_parent)),
            None => Err(Error::Missing(resolved_parent)),
        }
    }

    fn child_paths(&self, dir: &str) -> Vec<String> {
        self.entries
            .keys()
            .filter(|path| {
                if dir == "/" {
                    path.starts_with('/') && *path != "/"
                        && !path[1..].contains('/')
                } else {
                    path.strip_prefix(dir)
                        .is_some_and(|rest| rest.starts_with('/') && !rest[1..].contains('/'))
                }
            })
            .cloned()
            .collect()
    }

    fn is_empty_dir(&self, dir: &str) -> bool {
        self.child_paths(dir).is_empty()
    }

    fn remove_subtree(&mut self, path: &str) {
        let prefix = if path == "/" {
            "/".to_string()
        } else {
            format!("{}/", path)
        };
        self.entries
            .retain(|key, _| key != path && !key.starts_with(&prefix));
    }

    pub fn mkdir(&mut self, path: &str) -> Result<(), Error> {
        let canonical = Self::normalize(path, &self.cwd)?;
        if canonical == "/" {
            return Err(Error::Exists("/".to_string()));
        }
        let (parent, name) = self.resolve_parent(&canonical)?;
        let dest = Self::join_child(&parent, &name);
        if self.entries.contains_key(&dest) {
            return Err(Error::Exists(dest));
        }
        self.entries.insert(dest, Entry::Dir);
        Ok(())
    }

    pub fn write(&mut self, path: &str, data: &[u8]) -> Result<(), Error> {
        let canonical = Self::normalize(path, &self.cwd)?;
        let resolved = self.resolve(&canonical, true, true)?;
        match self.entries.get(&resolved) {
            Some(Entry::Dir) => Err(Error::IsDir(resolved)),
            Some(Entry::File(_)) => {
                self.entries
                    .insert(resolved, Entry::File(data.to_vec()));
                Ok(())
            }
            Some(Entry::Link(_)) => unreachable!(),
            None => {
                let (parent, _) = self.resolve_parent(&resolved)?;
                let target = Self::join_child(&parent, Self::parent(&resolved).1);
                self.entries.insert(target, Entry::File(data.to_vec()));
                Ok(())
            }
        }
    }

    pub fn read(&self, path: &str) -> Result<Vec<u8>, Error> {
        let canonical = Self::normalize(path, &self.cwd)?;
        let resolved = self.resolve(&canonical, true, false)?;
        match self.entries.get(&resolved) {
            Some(Entry::File(data)) => Ok(data.clone()),
            Some(Entry::Dir) => Err(Error::IsDir(resolved)),
            Some(Entry::Link(_)) => unreachable!(),
            None => Err(Error::Missing(resolved)),
        }
    }

    pub fn ls(&self, path: &str) -> Result<Vec<(String, Kind)>, Error> {
        let canonical = Self::normalize(path, &self.cwd)?;
        let resolved = self.resolve(&canonical, true, false)?;
        match self.entries.get(&resolved) {
            Some(Entry::Dir) => {
                let mut result = Vec::new();
                for child in self.child_paths(&resolved) {
                    let name = Self::parent(&child).1.to_string();
                    if let Some(entry) = self.entries.get(&child) {
                        result.push((name, Self::kind(entry)));
                    }
                }
                result.sort_by(|a, b| a.0.cmp(&b.0));
                Ok(result)
            }
            Some(_) => Err(Error::NotDir(resolved)),
            None => Err(Error::Missing(resolved)),
        }
    }

    pub fn cwd(&self) -> String {
        self.cwd.clone()
    }

    pub fn chdir(&mut self, path: &str) -> Result<(), Error> {
        let canonical = Self::normalize(path, &self.cwd)?;
        let resolved = self.resolve(&canonical, true, false)?;
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
        let source = Self::normalize(from, &self.cwd)?;
        let destination = Self::normalize(to, &self.cwd)?;
        if source == "/" || destination == "/" {
            return Err(Error::Root);
        }

        let (src_parent, src_name) = self.resolve_parent(&source)?;
        let src = Self::join_child(&src_parent, &src_name);
        let Some(src_entry) = self.entries.get(&src).cloned() else {
            return Err(Error::Missing(src));
        };

        if matches!(src_entry, Entry::Dir) && Self::is_within(&self.cwd, &src) {
            return Err(Error::Busy(src));
        }

        let (dst_parent, dst_name) = self.resolve_parent(&destination)?;
        let dst = Self::join_child(&dst_parent, &dst_name);

        if self
            .entries
            .get(&dst)
            .is_some_and(|entry| matches!(entry, Entry::Dir) && Self::is_within(&self.cwd, &dst))
        {
            return Err(Error::Busy(dst));
        }

        if src == dst {
            return Ok(());
        }

        if matches!(src_entry, Entry::Dir) && Self::is_within(&dst, &src) {
            return Err(Error::Cycle(dst));
        }

        let dst_entry = self.entries.get(&dst).cloned();
        if dst_entry.is_some() && !overwrite {
            return Err(Error::Exists(dst));
        }
        if let Some(existing) = &dst_entry {
            match (&src_entry, existing) {
                (Entry::Dir, Entry::Dir) => {
                    if !self.is_empty_dir(&dst) {
                        return Err(Error::NotEmpty(dst));
                    }
                }
                (Entry::Dir, _) => return Err(Error::NotDir(dst)),
                (_, Entry::Dir) => return Err(Error::IsDir(dst)),
                _ => {}
            }
        }

        let src_prefix = format!("{}/", src);
        let moved: Vec<(String, Entry)> = self
            .entries
            .iter()
            .filter(|(path, _)| *path == &src || path.starts_with(&src_prefix))
            .map(|(path, entry)| {
                let suffix = &path[src.len()..];
                (format!("{}{}", dst, suffix), entry.clone())
            })
            .collect();

        if dst_entry.is_some() {
            self.remove_subtree(&dst);
        }
        self.remove_subtree(&src);
        for (path, entry) in moved {
            self.entries.insert(path, entry);
        }
        Ok(())
    }

    pub fn remove(&mut self, path: &str, recursive: bool) -> Result<(), Error> {
        let canonical = Self::normalize(path, &self.cwd)?;
        if canonical == "/" {
            return Err(Error::Root);
        }
        let (parent, name) = self.resolve_parent(&canonical)?;
        let target = Self::join_child(&parent, &name);
        let Some(entry) = self.entries.get(&target) else {
            return Err(Error::Missing(target));
        };

        if matches!(entry, Entry::Dir) && Self::is_within(&self.cwd, &target) {
            return Err(Error::Busy(target));
        }
        if matches!(entry, Entry::Dir) && !recursive && !self.is_empty_dir(&target) {
            return Err(Error::NotEmpty(target));
        }

        self.remove_subtree(&target);
        Ok(())
    }

    pub fn symlink(&mut self, target: &str, link: &str) -> Result<(), Error> {
        let canonical = Self::normalize(link, &self.cwd)?;
        if target.is_empty() {
            return Err(Error::InvalidTarget(0));
        }
        if let Some(i) = target.as_bytes().iter().position(|&b| b == 0) {
            return Err(Error::InvalidTarget(i));
        }
        if canonical == "/" {
            return Err(Error::Exists("/".to_string()));
        }
        let (parent, name) = self.resolve_parent(&canonical)?;
        let dest = Self::join_child(&parent, &name);
        if self.entries.contains_key(&dest) {
            return Err(Error::Exists(dest));
        }
        self.entries.insert(dest, Entry::Link(target.to_string()));
        Ok(())
    }

    pub fn read_link(&self, path: &str) -> Result<String, Error> {
        let canonical = Self::normalize(path, &self.cwd)?;
        let (parent, name) = Self::parent(&canonical);
        let resolved_parent = self.resolve(parent, true, false)?;
        match self.entries.get(&resolved_parent) {
            Some(Entry::Dir) => {}
            Some(_) => return Err(Error::NotDir(resolved_parent)),
            None => return Err(Error::Missing(resolved_parent)),
        }
        let target = Self::join_child(&resolved_parent, name);
        match self.entries.get(&target) {
            Some(Entry::Link(text)) => Ok(text.clone()),
            Some(_) => Err(Error::InvalidTarget(0)),
            None => Err(Error::Missing(target)),
        }
    }

    pub fn du(&self, path: &str) -> Result<Vec<(String, u64)>, Error> {
        let canonical = Self::normalize(path, &self.cwd)?;
        let resolved = self.resolve(&canonical, true, false)?;
        let Some(entry) = self.entries.get(&resolved) else {
            return Err(Error::Missing(resolved));
        };

        let mut paths = vec![resolved.clone()];
        if matches!(entry, Entry::Dir) {
            let prefix = if resolved == "/" {
                "/".to_string()
            } else {
                format!("{}/", resolved)
            };
            paths.extend(
                self.entries
                    .keys()
                    .filter(|candidate| candidate.starts_with(&prefix))
                    .cloned(),
            );
        }
        paths.sort();

        let mut sizes = BTreeMap::new();
        for candidate in &paths {
            let size = match self.entries.get(candidate) {
                Some(Entry::File(data)) => data.len() as u64,
                Some(Entry::Link(target)) => target.len() as u64,
                Some(Entry::Dir) => 0,
                None => 0,
            };
            sizes.insert(candidate.clone(), size);
        }

        for candidate in paths.iter().rev() {
            if matches!(self.entries.get(candidate), Some(Entry::Dir)) {
                let prefix = if candidate == "/" {
                    "/".to_string()
                } else {
                    format!("{}/", candidate)
                };
                let total = paths
                    .iter()
                    .filter(|descendant| descendant.starts_with(&prefix))
                    .filter(|descendant| {
                        let rest = descendant.strip_prefix(&prefix).unwrap_or("");
                        !rest.is_empty()
                    })
                    .filter_map(|descendant| sizes.get(descendant))
                    .copied()
                    .sum();
                sizes.insert(candidate.clone(), total);
            }
        }

        Ok(paths
            .into_iter()
            .map(|candidate| {
                let size = *sizes.get(&candidate).unwrap_or(&0);
                (candidate, size)
            })
            .collect())
    }
}

fn main() {}