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

#[derive(Debug, Clone)]
struct Node {
    entry: Entry,
    children: std::collections::BTreeMap<String, Node>,
}

impl Node {
    fn dir() -> Self {
        Self {
            entry: Entry::Dir,
            children: std::collections::BTreeMap::new(),
        }
    }

    fn new(entry: Entry) -> Self {
        Self {
            entry,
            children: std::collections::BTreeMap::new(),
        }
    }

    fn kind(&self) -> Kind {
        match self.entry {
            Entry::Dir => Kind::Dir,
            Entry::File(_) => Kind::File,
            Entry::Link(_) => Kind::Link,
        }
    }
}

#[derive(Debug)]
pub struct Fs {
    root: Node,
    cwd: Vec<String>,
}

impl Default for Fs {
    fn default() -> Self {
        Self::new()
    }
}

impl Fs {
    pub fn new() -> Self {
        Self {
            root: Node::dir(),
            cwd: Vec::new(),
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

    fn normalize_from(path: &str, base: &[String]) -> Vec<String> {
        let mut parts = if path.starts_with('/') {
            Vec::new()
        } else {
            base.to_vec()
        };
        for component in path.split('/') {
            match component {
                "" | "." => {}
                ".." => {
                    parts.pop();
                }
                other => parts.push(other.to_owned()),
            }
        }
        parts
    }

    fn path_string(parts: &[String]) -> String {
        if parts.is_empty() {
            "/".to_owned()
        } else {
            format!("/{}", parts.join("/"))
        }
    }

    fn child<'a>(node: &'a Node, name: &str) -> Option<&'a Node> {
        if !matches!(node.entry, Entry::Dir) {
            None
        } else {
            node.children.get(name)
        }
    }

    fn child_mut<'a>(node: &'a mut Node, name: &str) -> Option<&'a mut Node> {
        if !matches!(node.entry, Entry::Dir) {
            None
        } else {
            node.children.get_mut(name)
        }
    }

    fn get_node<'a>(&'a self, parts: &[String]) -> Option<&'a Node> {
        let mut node = &self.root;
        for p in parts {
            node = Self::child(node, p)?;
        }
        Some(node)
    }

    fn get_node_mut<'a>(&'a mut self, parts: &[String]) -> Option<&'a mut Node> {
        let mut node = &mut self.root;
        for p in parts {
            node = Self::child_mut(node, p)?;
        }
        Some(node)
    }

    fn parent_path(parts: &[String]) -> Vec<String> {
        parts[..parts.len().saturating_sub(1)].to_vec()
    }

    fn resolve(
        &self,
        path: &[String],
        follow_final: bool,
        allow_missing_final: bool,
    ) -> Result<Vec<String>, Error> {
        let mut pending = path.to_vec();
        let mut resolved: Vec<String> = Vec::new();
        let mut seen = std::collections::HashSet::<String>::new();
        let mut expansions = 0usize;
        let mut idx = 0usize;

        loop {
            if idx == pending.len() {
                return Ok(resolved);
            }

            let component = pending[idx].clone();
            let mut candidate = resolved.clone();
            candidate.push(component.clone());
            let candidate_name = Self::path_string(&candidate);
            let parent_name = Self::path_string(&resolved);

            let parent = self.get_node(&resolved).ok_or_else(|| Error::Missing(parent_name.clone()))?;
            if !matches!(parent.entry, Entry::Dir) {
                return Err(Error::NotDir(parent_name));
            }

            let Some(node) = parent.children.get(&component) else {
                if allow_missing_final && idx + 1 == pending.len() {
                    return Ok(candidate);
                }
                return Err(Error::Missing(candidate_name));
            };

            let is_final = idx + 1 == pending.len();
            if let Entry::Link(target) = &node.entry {
                if !is_final || follow_final {
                    if !seen.insert(candidate_name.clone()) {
                        return Err(Error::Loop(candidate_name));
                    }
                    if expansions >= 8 {
                        return Err(Error::Depth(candidate_name));
                    }
                    expansions += 1;
                    let target_parts = Self::normalize_from(target, &resolved);
                    let suffix = pending[idx + 1..].to_vec();
                    let mut expanded = target_parts;
                    expanded.extend(suffix);
                    pending = Self::normalize_from(&Self::path_string(&expanded), &[]);
                    resolved.clear();
                    idx = 0;
                    continue;
                }
            }

            resolved.push(component);
            idx += 1;
        }
    }

    fn resolve_parent(&self, path: &[String]) -> Result<Vec<String>, Error> {
        let parent = Self::parent_path(path);
        self.resolve(&parent, true, false)
    }

    fn is_prefix(a: &[String], b: &[String]) -> bool {
        a.len() <= b.len() && a.iter().zip(b.iter()).all(|(x, y)| x == y)
    }

    fn nonempty(node: &Node) -> bool {
        matches!(node.entry, Entry::Dir) && !node.children.is_empty()
    }

    pub fn mkdir(&mut self, path: &str) -> Result<(), Error> {
        Self::validate(path)?;
        let canonical = Self::normalize_from(path, &self.cwd);
        if canonical.is_empty() {
            return Err(Error::Exists("/".to_owned()));
        }

        let parent = self.resolve_parent(&canonical)?;
        let name = canonical.last().unwrap().clone();
        let mut full = parent.clone();
        full.push(name.clone());
        let full_name = Self::path_string(&full);

        let parent_node = self.get_node(&parent).ok_or_else(|| Error::Missing(Self::path_string(&parent)))?;
        if !matches!(parent_node.entry, Entry::Dir) {
            return Err(Error::NotDir(Self::path_string(&parent)));
        }
        if parent_node.children.contains_key(&name) {
            return Err(Error::Exists(full_name));
        }

        self.get_node_mut(&parent)
            .unwrap()
            .children
            .insert(name, Node::dir());
        Ok(())
    }

    pub fn write(&mut self, path: &str, data: &[u8]) -> Result<(), Error> {
        Self::validate(path)?;
        let canonical = Self::normalize_from(path, &self.cwd);
        let resolved = self.resolve(&canonical, true, true)?;
        if resolved.is_empty() {
            return Err(Error::IsDir("/".to_owned()));
        }

        let parent = Self::parent_path(&resolved);
        let name = resolved.last().unwrap().clone();
        let path_name = Self::path_string(&resolved);
        let parent_node = self.get_node(&parent).ok_or_else(|| Error::Missing(Self::path_string(&parent)))?;
        if !matches!(parent_node.entry, Entry::Dir) {
            return Err(Error::NotDir(Self::path_string(&parent)));
        }

        if let Some(existing) = parent_node.children.get(&name) {
            if matches!(existing.entry, Entry::Dir) {
                return Err(Error::IsDir(path_name));
            }
        }

        let parent_node = self.get_node_mut(&parent).unwrap();
        match parent_node.children.get_mut(&name) {
            Some(existing) => existing.entry = Entry::File(data.to_vec()),
            None => {
                parent_node
                    .children
                    .insert(name, Node::new(Entry::File(data.to_vec())));
            }
        }
        Ok(())
    }

    pub fn read(&self, path: &str) -> Result<Vec<u8>, Error> {
        Self::validate(path)?;
        let canonical = Self::normalize_from(path, &self.cwd);
        let resolved = self.resolve(&canonical, true, false)?;
        let node = self.get_node(&resolved).ok_or_else(|| Error::Missing(Self::path_string(&resolved)))?;
        match &node.entry {
            Entry::File(data) => Ok(data.clone()),
            Entry::Dir => Err(Error::IsDir(Self::path_string(&resolved))),
            Entry::Link(_) => unreachable!(),
        }
    }

    pub fn ls(&self, path: &str) -> Result<Vec<(String, Kind)>, Error> {
        Self::validate(path)?;
        let canonical = Self::normalize_from(path, &self.cwd);
        let resolved = self.resolve(&canonical, true, false)?;
        let node = self.get_node(&resolved).ok_or_else(|| Error::Missing(Self::path_string(&resolved)))?;
        if !matches!(node.entry, Entry::Dir) {
            return Err(Error::NotDir(Self::path_string(&resolved)));
        }
        Ok(node
            .children
            .iter()
            .map(|(name, child)| (name.clone(), child.kind()))
            .collect())
    }

    pub fn cwd(&self) -> String {
        Self::path_string(&self.cwd)
    }

    pub fn chdir(&mut self, path: &str) -> Result<(), Error> {
        Self::validate(path)?;
        let canonical = Self::normalize_from(path, &self.cwd);
        let resolved = self.resolve(&canonical, true, false)?;
        let node = self.get_node(&resolved).ok_or_else(|| Error::Missing(Self::path_string(&resolved)))?;
        if !matches!(node.entry, Entry::Dir) {
            return Err(Error::NotDir(Self::path_string(&resolved)));
        }
        self.cwd = resolved;
        Ok(())
    }

    pub fn rename(&mut self, from: &str, to: &str, overwrite: bool) -> Result<(), Error> {
        Self::validate(from)?;
        Self::validate(to)?;
        let source = Self::normalize_from(from, &self.cwd);
        let destination = Self::normalize_from(to, &self.cwd);
        if source.is_empty() || destination.is_empty() {
            return Err(Error::Root);
        }

        let src_parent = self.resolve_parent(&source)?;
        let src_name = source.last().unwrap().clone();
        let mut src_path = src_parent.clone();
        src_path.push(src_name.clone());
        let src_node = self
            .get_node(&src_parent)
            .and_then(|n| n.children.get(&src_name))
            .ok_or_else(|| Error::Missing(Self::path_string(&src_path)))?;

        if matches!(src_node.entry, Entry::Dir) && Self::is_prefix(&src_path, &self.cwd) {
            return Err(Error::Busy(Self::path_string(&src_path)));
        }

        let dst_parent = self.resolve_parent(&destination)?;
        let dst_name = destination.last().unwrap().clone();
        let mut dst_path = dst_parent.clone();
        dst_path.push(dst_name.clone());

        if let Some(dst_node) = self
            .get_node(&dst_parent)
            .and_then(|n| n.children.get(&dst_name))
        {
            if matches!(dst_node.entry, Entry::Dir) && Self::is_prefix(&dst_path, &self.cwd) {
                return Err(Error::Busy(Self::path_string(&dst_path)));
            }
        }

        if src_path == dst_path {
            return Ok(());
        }

        if matches!(src_node.entry, Entry::Dir) && Self::is_prefix(&src_path, &dst_path) {
            return Err(Error::Cycle(Self::path_string(&dst_path)));
        }

        let dst_exists = self
            .get_node(&dst_parent)
            .and_then(|n| n.children.get(&dst_name))
            .is_some();
        if dst_exists && !overwrite {
            return Err(Error::Exists(Self::path_string(&dst_path)));
        }

        if dst_exists {
            let src_is_dir = matches!(src_node.entry, Entry::Dir);
            let dst_node = self.get_node(&dst_parent).unwrap().children.get(&dst_name).unwrap();
            let dst_is_dir = matches!(dst_node.entry, Entry::Dir);
            if src_is_dir && !dst_is_dir {
                return Err(Error::NotDir(Self::path_string(&dst_path)));
            }
            if !src_is_dir && dst_is_dir {
                return Err(Error::IsDir(Self::path_string(&dst_path)));
            }
            if src_is_dir && dst_is_dir && Self::nonempty(dst_node) {
                return Err(Error::NotEmpty(Self::path_string(&dst_path)));
            }
        }

        let mut moved = self
            .get_node_mut(&src_parent)
            .unwrap()
            .children
            .remove(&src_name)
            .unwrap();

        let dst_parent_node = self.get_node_mut(&dst_parent).unwrap();
        if dst_exists {
            dst_parent_node.children.remove(&dst_name);
        }
        dst_parent_node.children.insert(dst_name, moved);
        Ok(())
    }

    pub fn remove(&mut self, path: &str, recursive: bool) -> Result<(), Error> {
        Self::validate(path)?;
        let canonical = Self::normalize_from(path, &self.cwd);
        if canonical.is_empty() {
            return Err(Error::Root);
        }

        let parent = self.resolve_parent(&canonical)?;
        let name = canonical.last().unwrap().clone();
        let mut full = parent.clone();
        full.push(name.clone());
        let full_name = Self::path_string(&full);

        let node = self
            .get_node(&parent)
            .and_then(|n| n.children.get(&name))
            .ok_or_else(|| Error::Missing(full_name.clone()))?;

        if matches!(node.entry, Entry::Dir) && Self::is_prefix(&full, &self.cwd) {
            return Err(Error::Busy(full_name));
        }
        if matches!(node.entry, Entry::Dir) && !recursive && Self::nonempty(node) {
            return Err(Error::NotEmpty(full_name));
        }

        self.get_node_mut(&parent).unwrap().children.remove(&name);
        Ok(())
    }

    pub fn symlink(&mut self, target: &str, link: &str) -> Result<(), Error> {
        Self::validate(link)?;
        let link_path = Self::normalize_from(link, &self.cwd);
        Self::validate(target).map_err(|e| match e {
            Error::InvalidPath(i) => Error::InvalidTarget(i),
            _ => Error::InvalidTarget(0),
        })?;

        if link_path.is_empty() {
            return Err(Error::Exists("/".to_owned()));
        }

        let parent = self.resolve_parent(&link_path)?;
        let name = link_path.last().unwrap().clone();
        let mut full = parent.clone();
        full.push(name.clone());
        let full_name = Self::path_string(&full);

        let parent_node = self.get_node(&parent).ok_or_else(|| Error::Missing(Self::path_string(&parent)))?;
        if !matches!(parent_node.entry, Entry::Dir) {
            return Err(Error::NotDir(Self::path_string(&parent)));
        }
        if parent_node.children.contains_key(&name) {
            return Err(Error::Exists(full_name));
        }

        self.get_node_mut(&parent)
            .unwrap()
            .children
            .insert(name, Node::new(Entry::Link(target.to_owned())));
        Ok(())
    }

    pub fn read_link(&self, path: &str) -> Result<String, Error> {
        Self::validate(path)?;
        let canonical = Self::normalize_from(path, &self.cwd);
        if canonical.is_empty() {
            return Err(Error::InvalidTarget(0));
        }
        let parent = self.resolve_parent(&canonical)?;
        let name = canonical.last().unwrap();
        let full = {
            let mut p = parent.clone();
            p.push(name.clone());
            p
        };
        let node = self
            .get_node(&parent)
            .and_then(|n| n.children.get(name))
            .ok_or_else(|| Error::Missing(Self::path_string(&full)))?;
        match &node.entry {
            Entry::Link(target) => Ok(target.clone()),
            _ => Err(Error::InvalidTarget(0)),
        }
    }

    pub fn du(&self, path: &str) -> Result<Vec<(String, u64)>, Error> {
        Self::validate(path)?;
        let canonical = Self::normalize_from(path, &self.cwd);
        let resolved = self.resolve(&canonical, true, false)?;
        let root_node = self.get_node(&resolved).ok_or_else(|| Error::Missing(Self::path_string(&resolved)))?;

        fn gather(node: &Node, path: Vec<String>, rows: &mut Vec<(String, u64)>) -> u64 {
            let size = match &node.entry {
                Entry::File(data) => data.len() as u64,
                Entry::Link(target) => target.len() as u64,
                Entry::Dir => node
                    .children
                    .iter()
                    .map(|(name, child)| {
                        let mut p = path.clone();
                        p.push(name.clone());
                        gather(child, p, rows)
                    })
                    .sum(),
            };
            rows.push((Fs::path_string(&path), size));
            size
        }

        let mut rows = Vec::new();
        gather(root_node, resolved, &mut rows);
        rows.sort_by(|a, b| a.0.cmp(&b.0));
        Ok(rows)
    }
}

fn main() {}