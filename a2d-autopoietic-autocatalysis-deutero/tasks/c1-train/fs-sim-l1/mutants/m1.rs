use std::collections::{BTreeMap, BTreeSet};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind { Dir, File  }
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Error { InvalidPath(usize), Missing(String), NotDir(String), IsDir(String), Exists(String) }
#[derive(Clone)]
enum Node { Dir, File(Vec<u8>)  }
impl Node {
    fn kind(&self) -> Kind {
        match self { Node::Dir => Kind::Dir, Node::File(_) => Kind::File  }
    }
}
pub struct Fs { nodes: BTreeMap<String, Node>, cwd: String }
fn components(p: &str) -> Vec<String> {
    p.split('/').filter(|s| !s.is_empty()).map(str::to_owned).collect()
}
fn absolute(parts: &[String]) -> String {
    if parts.is_empty() { "/".to_owned() } else { format!("/{}", parts.join("/")) }
}
fn parent(p: &str) -> String {
    let mut parts = components(p); parts.pop(); absolute(&parts)
}
fn inside(p: &str, ancestor: &str) -> bool {
    p == ancestor || ancestor == "/" || p.strip_prefix(ancestor).is_some_and(|s| s.starts_with('/'))
}
fn lexical(p: &str, base: &str) -> String {
    let mut parts = if p.starts_with('/') { Vec::new() } else { components(base) };
    for s in p.split('/') {
        match s { "" | "." => {}, ".." => { parts.pop(); }, _ => parts.push(s.to_owned()) }
    }
    absolute(&parts)
}
impl Fs {
    pub fn new() -> Self {
        let mut nodes = BTreeMap::new(); nodes.insert("/".to_owned(), Node::Dir);
        Self { nodes, cwd: "/".to_owned() }
    }
    fn normalize(&self, path: &str) -> Result<String, Error> {
        if let Some(i) = path.chars().position(|b| b == '\0') { return Err(Error::InvalidPath(i)); }
        if path.is_empty() { return Err(Error::InvalidPath(0)); }
        if !path.starts_with('/') { return Err(Error::InvalidPath(0)); }
        if path == "/" { return Ok(path.to_owned()); }
        let mut offset = 1;
        for s in path[1..].split('/') {
            if s.is_empty() { return Err(Error::InvalidPath(offset.min(path.len() - 1))); }
            if s == "." || s == ".." { return Err(Error::InvalidPath(offset)); }
            offset += s.len() + 1;
        }
        Ok(path.to_owned())
    }
    fn resolve(&self, path: &str, follow_final: bool, allow_missing_leaf: bool) -> Result<String, Error> {
        let _ = follow_final;
        let parts = components(path); let mut prefix = Vec::new();
        for (i, part) in parts.iter().enumerate() {
            prefix.push(part.clone()); let p = absolute(&prefix);
            match self.nodes.get(&p) {
                None if allow_missing_leaf && i + 1 == parts.len() => return Ok(p),
                None => return Err(Error::Missing(p)),
                Some(n) if i + 1 < parts.len() && n.kind() != Kind::Dir => return Err(Error::NotDir(p)),
                _ => {}
            }
        }
        Ok(path.to_owned())
    }
    fn checked_parent(&self, p: &str) -> Result<String, Error> {
        let q = self.resolve(&parent(p), true, false)?;
        if self.nodes[&q].kind() != Kind::Dir { return Err(Error::NotDir(q)); }
        Ok(q)
    }
    fn slot(&self, p: &str) -> Result<String, Error> {
        if p == "/" { return Ok(p.to_owned()); }
        let q = self.checked_parent(p)?;
        let name = components(p).pop().unwrap();
        Ok(if q == "/" { format!("/{name}") } else { format!("{q}/{name}") })
    }
    pub fn mkdir(&mut self, path: &str) -> Result<(), Error> {
        let p = self.normalize(path)?; let p = self.slot(&p)?;
        if self.nodes.contains_key(&p) { return Err(Error::Exists(p)); }
        self.nodes.insert(p, Node::Dir); Ok(())
    }
    pub fn write(&mut self, path: &str, data: &[u8]) -> Result<(), Error> {
        let p = self.normalize(path)?;
        let p = self.resolve(&p, true, true)?;
        if self.nodes.get(&p).is_some_and(|n| n.kind() == Kind::Dir) { return Err(Error::IsDir(p)); }
        self.nodes.insert(p, Node::File(data.to_vec())); Ok(())
    }
    pub fn read(&self, path: &str) -> Result<Vec<u8>, Error> {
        let p = self.normalize(path)?; let p = self.resolve(&p, true, false)?;
        match &self.nodes[&p] { Node::File(data) => Ok(data.clone()), _ => Err(Error::IsDir(p)) }
    }
    pub fn ls(&self, path: &str) -> Result<Vec<(String, Kind)>, Error> {
        let p = self.normalize(path)?; let p = self.resolve(&p, true, false)?;
        if self.nodes[&p].kind() != Kind::Dir { return Err(Error::NotDir(p)); }
        let mut entries = Vec::new();
        for (q, n) in &self.nodes {
            if q != "/" && parent(q) == p { entries.push((components(q).pop().unwrap(), n.kind())); }
        }
        entries.sort_by(|a, b| a.0.cmp(&b.0)); Ok(entries)
    }



}
