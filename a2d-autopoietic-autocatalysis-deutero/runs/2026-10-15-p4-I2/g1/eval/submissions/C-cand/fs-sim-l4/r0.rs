use std::collections::{BTreeMap, BTreeSet, VecDeque};

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
enum Node {
    Dir,
    File(Vec<u8>),
    Link(String),
}

pub struct Fs {
    entries: BTreeMap<String, Node>,
    cwd: String,
}

fn canonical(parts: &[String]) -> String {
    if parts.is_empty() {
        "/".to_string()
    } else {
        format!("/{}", parts.join("/"))
    }
}

fn parts_of(path: &str) -> Vec<String> {
    if path == "/" {
        Vec::new()
    } else {
        path[1..].split('/').map(str::to_string).collect()
    }
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
        parts_of(cwd)
    };
    for comp in path.split('/') {
        match comp {
            "" | "." => {}
            ".." => {
                parts.pop();
            }
            _ => parts.push(comp.to_string()),
        }
    }
    Ok(canonical(&parts))
}

fn join_child(parent: &str, name: &str) -> String {
    if parent == "/" {
        format!("/{name}")
    } else {
        format!("{parent}/{name}")
    }
}

fn parent_and_name(path: &str) -> (String, String) {
    if path == "/" {
        return ("/".to_string(), String::new());
    }
    let i = path.rfind('/').unwrap();
    let parent = if i == 0 { "/" } else { &path[..i] };
    (parent.to_string(), path[i + 1..].to_string())
}

fn is_within(path: &str, dir: &str) -> bool {
    path == dir || if dir == "/" {
        path.starts_with('/')
    } else {
        path.strip_prefix(dir)
            .map(|s| s.starts_with('/'))
            .unwrap_or(false)
    }
}

impl Fs {
    pub fn new() -> Self {
        let mut entries = BTreeMap::new();
        entries.insert("/".to_string(), Node::Dir);
        Self {
            entries,
            cwd: "/".to_string(),
        }
    }

    fn resolve(&self, path: &str, allow_missing_final: bool) -> Result<(String, bool), Error> {
        let mut todo: VecDeque<String> = parts_of(path).into();
        let mut done: Vec<String> = Vec::new();
        let mut visited = BTreeSet::new();
        let mut expansions = 0usize;

        loop {
            let Some(comp) = todo.pop_front() else {
                return Ok((canonical(&done), true));
            };
            let candidate = {
                let mut p = done.clone();
                p.push(comp.clone());
                canonical(&p)
            };
            let is_final = todo.is_empty();
            match self.entries.get(&candidate) {
                None => {
                    if allow_missing_final && is_final {
                        return Ok((candidate, false));
                    }
                    return Err(Error::Missing(candidate));
                }
                Some(Node::Dir) => done.push(comp),
                Some(Node::File(_)) => {
                    if !is_final {
                        return Err(Error::NotDir(candidate));
                    }
                    done.push(comp);
                }
                Some(Node::Link(target)) => {
                    if !visited.insert(candidate.clone()) {
                        return Err(Error::Loop(candidate));
                    }
                    if expansions >= 8 {
                        return Err(Error::Depth(candidate));
                    }
                    expansions += 1;

                    let (link_parent, _) = parent_and_name(&candidate);
                    let target_path = normalize(target, &link_parent)
                        .map_err(|e| match e {
                            Error::InvalidPath(i) => Error::InvalidTarget(i),
                            other => other,
                        })?;
                    let mut expanded: VecDeque<String> = parts_of(&target_path).into();
                    expanded.append(&mut todo);
                    todo = expanded;
                    done.clear();
                }
            }
        }
    }

    fn resolve_parent(&self, path: &str) -> Result<(String, String), Error> {
        if path == "/" {
            return Ok(("/".to_string(), String::new()));
        }
        let (parent, name) = parent_and_name(path);
        let (resolved_parent, _) = self.resolve(&parent, false)?;
        match self.entries.get(&resolved_parent) {
            Some(Node::Dir) => Ok((resolved_parent, name)),
            Some(_) => Err(Error::NotDir(resolved_parent)),
            None => Err(Error::Missing(resolved_parent)),
        }
    }

    fn kind(node: &Node) -> Kind {
        match node {
            Node::Dir => Kind::Dir,
            Node::File(_) => Kind::File,
            Node::Link(_) => Kind::Link,
        }
    }

    fn subtree_paths(&self, root: &str) -> Vec<String> {
        self.entries
            .keys()
            .filter(|p| is_within(p, root))
            .cloned()
            .collect()
    }

    pub fn mkdir(&mut self, path: &str) -> Result<(), Error> {
        let normalized = normalize(path, &self.cwd)?;
        let (parent, name) = self.resolve_parent(&normalized)?;
        let dest = if normalized == "/" {
            "/".to_string()
        } else {
            join_child(&parent, &name)
        };
        if self.entries.contains_key(&dest) {
            return Err(Error::Exists(dest));
        }
        self.entries.insert(dest, Node::Dir);
        Ok(())
    }

    pub fn write(&mut self, path: &str, data: &[u8]) -> Result<(), Error> {
        let normalized = normalize(path, &self.cwd)?;
        let (resolved, exists) = self.resolve(&normalized, true)?;
        if exists {
            match self.entries.get(&resolved) {
                Some(Node::Dir) => return Err(Error::IsDir(resolved)),
                Some(Node::File(_)) => {}
                Some(Node::Link(_)) => unreachable!(),
                None => return Err(Error::Missing(resolved)),
            }
        } else {
            let (parent, _) = parent_and_name(&resolved);
            match self.entries.get(&parent) {
                Some(Node::Dir) => {}
                Some(_) => return Err(Error::NotDir(parent)),
                None => return Err(Error::Missing(parent)),
            }
        }
        self.entries
            .insert(resolved, Node::File(data.to_vec()));
        Ok(())
    }

    pub fn read(&self, path: &str) -> Result<Vec<u8>, Error> {
        let normalized = normalize(path, &self.cwd)?;
        let (resolved, _) = self.resolve(&normalized, false)?;
        match self.entries.get(&resolved) {
            Some(Node::File(bytes)) => Ok(bytes.clone()),
            Some(Node::Dir) => Err(Error::IsDir(resolved)),
            Some(Node::Link(_)) => unreachable!(),
            None => Err(Error::Missing(resolved)),
        }
    }

    pub fn ls(&self, path: &str) -> Result<Vec<(String, Kind)>, Error> {
        let normalized = normalize(path, &self.cwd)?;
        let (resolved, _) = self.resolve(&normalized, false)?;
        match self.entries.get(&resolved) {
            Some(Node::Dir) => {}
            Some(_) => return Err(Error::NotDir(resolved)),
            None => return Err(Error::Missing(resolved)),
        }
        let mut out = Vec::new();
        for (entry, node) in &self.entries {
            if entry == &resolved || !is_within(entry, &resolved) {
                continue;
            }
            let (parent, name) = parent_and_name(entry);
            if parent == resolved {
                out.push((name, Self::kind(node)));
            }
        }
        out.sort_by(|a, b| a.0.cmp(&b.0));
        Ok(out)
    }

    pub fn cwd(&self) -> String {
        self.cwd.clone()
    }

    pub fn chdir(&mut self, path: &str) -> Result<(), Error> {
        let normalized = normalize(path, &self.cwd)?;
        let (resolved, _) = self.resolve(&normalized, false)?;
        match self.entries.get(&resolved) {
            Some(Node::Dir) => {
                self.cwd = resolved;
                Ok(())
            }
            Some(_) => Err(Error::NotDir(resolved)),
            None => Err(Error::Missing(resolved)),
        }
    }

    pub fn rename(
        &mut self,
        from: &str,
        to: &str,
        overwrite: bool,
    ) -> Result<(), Error> {
        let source_norm = normalize(from, &self.cwd)?;
        let dest_norm = normalize(to, &self.cwd)?;
        if source_norm == "/" || dest_norm == "/" {
            return Err(Error::Root);
        }

        let (source_parent, source_name) = self.resolve_parent(&source_norm)?;
        let source = join_child(&source_parent, &source_name);
        let source_node = self
            .entries
            .get(&source)
            .cloned()
            .ok_or_else(|| Error::Missing(source.clone()))?;
        if matches!(source_node, Node::Dir) && is_within(&self.cwd, &source) {
            return Err(Error::Busy(source));
        }

        let (dest_parent, dest_name) = self.resolve_parent(&dest_norm)?;
        let dest = join_child(&dest_parent, &dest_name);
        if matches!(self.entries.get(&dest), Some(Node::Dir))
            && is_within(&self.cwd, &dest)
        {
            return Err(Error::Busy(dest));
        }

        if source == dest {
            return Ok(());
        }
        if matches!(source_node, Node::Dir) && is_within(&dest, &source) {
            return Err(Error::Cycle(dest));
        }

        let dest_node = self.entries.get(&dest).cloned();
        if dest_node.is_some() && !overwrite {
            return Err(Error::Exists(dest));
        }

        if let Some(ref dn) = dest_node {
            match (&source_node, dn) {
                (Node::Dir, Node::Dir) => {
                    if self.subtree_paths(&dest).len() > 1 {
                        return Err(Error::NotEmpty(dest));
                    }
                }
                (Node::Dir, _) => return Err(Error::NotDir(dest)),
                (_, Node::Dir) => return Err(Error::IsDir(dest)),
                _ => {}
            }
        }

        let source_paths = self.subtree_paths(&source);
        let dest_paths = if dest_node.is_some() {
            self.subtree_paths(&dest)
        } else {
            Vec::new()
        };

        for p in dest_paths {
            self.entries.remove(&p);
        }

        let moved: Vec<(String, Node)> = source_paths
            .iter()
            .filter_map(|p| self.entries.get(p).cloned().map(|n| (p.clone(), n)))
            .collect();
        for p in &source_paths {
            self.entries.remove(p);
        }
        for (old, node) in moved {
            let suffix = if old == source {
                String::new()
            } else {
                old[source.len()..].to_string()
            };
            self.entries.insert(format!("{dest}{suffix}"), node);
        }
        Ok(())
    }

    pub fn remove(&mut self, path: &str, recursive: bool) -> Result<(), Error> {
        let normalized = normalize(path, &self.cwd)?;
        if normalized == "/" {
            return Err(Error::Root);
        }
        let (parent, name) = self.resolve_parent(&normalized)?;
        let resolved = join_child(&parent, &name);
        let node = self
            .entries
            .get(&resolved)
            .cloned()
            .ok_or_else(|| Error::Missing(resolved.clone()))?;
        if matches!(node, Node::Dir) && is_within(&self.cwd, &resolved) {
            return Err(Error::Busy(resolved));
        }
        let paths = self.subtree_paths(&resolved);
        if matches!(node, Node::Dir) && paths.len() > 1 && !recursive {
            return Err(Error::NotEmpty(resolved));
        }
        for p in paths {
            self.entries.remove(&p);
        }
        Ok(())
    }

    pub fn symlink(&mut self, target: &str, link: &str) -> Result<(), Error> {
        let normalized_link = normalize(link, &self.cwd)?;
        if let Some(i) = target.as_bytes().iter().position(|b| *b == 0) {
            return Err(Error::InvalidTarget(i));
        }
        if target.is_empty() {
            return Err(Error::InvalidTarget(0));
        }
        if normalized_link == "/" {
            return Err(Error::Exists("/".to_string()));
        }
        let (parent, name) = self.resolve_parent(&normalized_link)?;
        let resolved = join_child(&parent, &name);
        if self.entries.contains_key(&resolved) {
            return Err(Error::Exists(resolved));
        }
        self.entries.insert(resolved, Node::Link(target.to_string()));
        Ok(())
    }

    pub fn read_link(&self, path: &str) -> Result<String, Error> {
        let normalized = normalize(path, &self.cwd)?;
        if normalized == "/" {
            return Err(Error::InvalidTarget(0));
        }
        let (parent, name) = self.resolve_parent(&normalized)?;
        let resolved = join_child(&parent, &name);
        match self.entries.get(&resolved) {
            Some(Node::Link(target)) => Ok(target.clone()),
            Some(_) => Err(Error::InvalidTarget(0)),
            None => Err(Error::Missing(resolved)),
        }
    }

    pub fn du(&self, path: &str) -> Result<Vec<(String, u64)>, Error> {
        let normalized = normalize(path, &self.cwd)?;
        let (resolved, _) = self.resolve(&normalized, false)?;
        let node = self
            .entries
            .get(&resolved)
            .ok_or_else(|| Error::Missing(resolved.clone()))?;
        if !matches!(node, Node::Dir | Node::File(_) | Node::Link(_)) {
            return Err(Error::Missing(resolved));
        }

        let paths = self.subtree_paths(&resolved);
        let mut sizes = BTreeMap::new();
        for p in &paths {
            let size = match self.entries.get(p).unwrap() {
                Node::Dir => 0,
                Node::File(bytes) => bytes.len() as u64,
                Node::Link(target) => target.len() as u64,
            };
            sizes.insert(p.clone(), size);
        }

        let mut rows = Vec::new();
        for p in paths {
            let total = if matches!(self.entries.get(&p), Some(Node::Dir)) {
                sizes
                    .iter()
                    .filter(|(child, _)| child.as_str() != p && is_within(child, &p))
                    .map(|(_, size)| *size)
                    .sum()
            } else {
                *sizes.get(&p).unwrap()
            };
            rows.push((p, total));
        }
        rows.sort_by(|a, b| a.0.cmp(&b.0));
        Ok(rows)
    }
}

impl Default for Fs {
    fn default() -> Self {
        Self::new()
    }
}

fn main() {}