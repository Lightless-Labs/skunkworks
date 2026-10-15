#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    Dir,
    File,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Error {
    InvalidPath(usize),
    Missing(String),
    NotDir(String),
    IsDir(String),
    Exists(String),
}

enum Node {
    Dir(std::collections::BTreeMap<String, Node>),
    File(Vec<u8>),
}

pub struct Fs {
    root: Node,
}

impl Fs {
    pub fn new() -> Self {
        Self {
            root: Node::Dir(std::collections::BTreeMap::new()),
        }
    }

    fn components(path: &str) -> Result<Vec<&str>, Error> {
        if let Some(i) = path.as_bytes().iter().position(|&b| b == 0) {
            return Err(Error::InvalidPath(i));
        }
        if path.is_empty() || !path.starts_with('/') {
            return Err(Error::InvalidPath(0));
        }
        if path == "/" {
            return Ok(Vec::new());
        }

        let mut parts = Vec::new();
        let mut start = 1;
        for (i, part) in path[1..].split('/').enumerate() {
            let offset = if i == 0 {
                start
            } else {
                start
            };
            if part.is_empty() {
                let error_offset = if offset == path.len() {
                    path.len() - 1
                } else {
                    offset - 1
                };
                return Err(Error::InvalidPath(error_offset));
            }
            if part == "." || part == ".." {
                return Err(Error::InvalidPath(offset));
            }
            parts.push(part);
            start = offset + part.len() + 1;
        }
        Ok(parts)
    }

    fn canonical(parts: &[&str]) -> String {
        if parts.is_empty() {
            "/".to_owned()
        } else {
            let mut result = String::new();
            for part in parts {
                result.push('/');
                result.push_str(part);
            }
            result
        }
    }

    fn find<'a>(root: &'a Node, parts: &[&str]) -> Result<&'a Node, Error> {
        let mut node = root;
        let mut prefix = Vec::new();

        for part in parts {
            match node {
                Node::Dir(children) => {
                    prefix.push(*part);
                    node = children
                        .get(*part)
                        .ok_or_else(|| Error::Missing(Self::canonical(&prefix)))?;
                }
                Node::File(_) => return Err(Error::NotDir(Self::canonical(&prefix))),
            }
        }
        Ok(node)
    }

    fn parent_mut<'a>(
        root: &'a mut Node,
        parent_parts: &[&str],
    ) -> Result<&'a mut std::collections::BTreeMap<String, Node>, Error> {
        let mut node = root;
        let mut prefix = Vec::new();

        for part in parent_parts {
            match node {
                Node::Dir(children) => {
                    prefix.push(*part);
                    node = children
                        .get_mut(*part)
                        .ok_or_else(|| Error::Missing(Self::canonical(&prefix)))?;
                }
                Node::File(_) => return Err(Error::NotDir(Self::canonical(&prefix))),
            }
        }

        match node {
            Node::Dir(children) => Ok(children),
            Node::File(_) => Err(Error::NotDir(Self::canonical(parent_parts))),
        }
    }

    pub fn mkdir(&mut self, path: &str) -> Result<(), Error> {
        let parts = Self::components(path)?;
        if parts.is_empty() {
            return Err(Error::Exists("/".to_owned()));
        }

        let parent_parts = &parts[..parts.len() - 1];
        let name = parts[parts.len() - 1];
        let canonical = Self::canonical(&parts);
        let parent = Self::parent_mut(&mut self.root, parent_parts)?;
        if parent.contains_key(name) {
            return Err(Error::Exists(canonical));
        }
        parent.insert(name.to_owned(), Node::Dir(std::collections::BTreeMap::new()));
        Ok(())
    }

    pub fn write(&mut self, path: &str, data: &[u8]) -> Result<(), Error> {
        let parts = Self::components(path)?;
        if parts.is_empty() {
            return Err(Error::IsDir("/".to_owned()));
        }

        let parent_parts = &parts[..parts.len() - 1];
        let name = parts[parts.len() - 1];
        let canonical = Self::canonical(&parts);
        let parent = Self::parent_mut(&mut self.root, parent_parts)?;

        match parent.get_mut(name) {
            Some(Node::Dir(_)) => Err(Error::IsDir(canonical)),
            Some(Node::File(contents)) => {
                contents.clear();
                contents.extend_from_slice(data);
                Ok(())
            }
            None => {
                parent.insert(name.to_owned(), Node::File(data.to_vec()));
                Ok(())
            }
        }
    }

    pub fn read(&self, path: &str) -> Result<Vec<u8>, Error> {
        let parts = Self::components(path)?;
        match Self::find(&self.root, &parts)? {
            Node::File(contents) => Ok(contents.clone()),
            Node::Dir(_) => Err(Error::IsDir(Self::canonical(&parts))),
        }
    }

    pub fn ls(&self, path: &str) -> Result<Vec<(String, Kind)>, Error> {
        let parts = Self::components(path)?;
        match Self::find(&self.root, &parts)? {
            Node::File(_) => Err(Error::NotDir(Self::canonical(&parts))),
            Node::Dir(children) => Ok(children
                .iter()
                .map(|(name, node)| {
                    let kind = match node {
                        Node::Dir(_) => Kind::Dir,
                        Node::File(_) => Kind::File,
                    };
                    (name.clone(), kind)
                })
                .collect()),
        }
    }
}

fn main() {}