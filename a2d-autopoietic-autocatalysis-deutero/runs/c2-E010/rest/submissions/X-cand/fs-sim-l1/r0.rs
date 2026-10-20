use std::collections::BTreeMap;

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
    Directory(BTreeMap<String, Node>),
    RegularFile(Vec<u8>),
}

pub struct Fs {
    root: Node,
}

impl Fs {
    pub fn new() -> Self {
        Self {
            root: Node::Directory(BTreeMap::new()),
        }
    }

    fn parse(path: &str) -> Result<Vec<&str>, Error> {
        if let Some(offset) = path.as_bytes().iter().position(|byte| *byte == 0) {
            return Err(Error::InvalidPath(offset));
        }
        if path.is_empty() || path.as_bytes()[0] != b'/' {
            return Err(Error::InvalidPath(0));
        }
        if path == "/" {
            return Ok(Vec::new());
        }

        let mut components = Vec::new();
        let mut component_start = 1usize;
        for (offset, byte) in path.bytes().enumerate().skip(1) {
            if byte == b'/' {
                if offset == component_start {
                    return Err(Error::InvalidPath(offset));
                }
                let component = &path[component_start..offset];
                if component == "." || component == ".." {
                    return Err(Error::InvalidPath(component_start));
                }
                components.push(component);
                component_start = offset + 1;
            }
        }

        if component_start == path.len() {
            return Err(Error::InvalidPath(path.len() - 1));
        }
        let final_component = &path[component_start..];
        if final_component == "." || final_component == ".." {
            return Err(Error::InvalidPath(component_start));
        }
        components.push(final_component);
        Ok(components)
    }

    fn path_for(components: &[&str]) -> String {
        let mut result = String::new();
        if components.is_empty() {
            result.push('/');
        } else {
            for component in components {
                result.push('/');
                result.push_str(component);
            }
        }
        result
    }

    fn lookup<'a>(root: &'a Node, components: &[&str]) -> Result<&'a Node, Error> {
        let mut node = root;
        let mut prefix = Vec::new();

        for component in components {
            match node {
                Node::RegularFile(_) => {
                    return Err(Error::NotDir(Self::path_for(&prefix)));
                }
                Node::Directory(children) => {
                    prefix.push(*component);
                    node = match children.get(*component) {
                        Some(found) => found,
                        None => return Err(Error::Missing(Self::path_for(&prefix))),
                    };
                }
            }
        }
        Ok(node)
    }

    fn parent_mut<'a>(
        root: &'a mut Node,
        components: &[&str],
    ) -> Result<&'a mut BTreeMap<String, Node>, Error> {
        let parents = &components[..components.len() - 1];
        let mut node = root;
        let mut prefix = Vec::new();

        for component in parents {
            match node {
                Node::RegularFile(_) => {
                    return Err(Error::NotDir(Self::path_for(&prefix)));
                }
                Node::Directory(children) => {
                    prefix.push(*component);
                    node = match children.get_mut(*component) {
                        Some(found) => found,
                        None => return Err(Error::Missing(Self::path_for(&prefix))),
                    };
                }
            }
        }

        match node {
            Node::Directory(children) => Ok(children),
            Node::RegularFile(_) => Err(Error::NotDir(Self::path_for(parents))),
        }
    }

    pub fn mkdir(&mut self, path: &str) -> Result<(), Error> {
        let components = Self::parse(path)?;
        if components.is_empty() {
            return Err(Error::Exists("/".to_owned()));
        }

        let parent = Self::parent_mut(&mut self.root, &components)?;
        let name = components[components.len() - 1];
        if parent.contains_key(name) {
            return Err(Error::Exists(Self::path_for(&components)));
        }
        parent.insert(name.to_owned(), Node::Directory(BTreeMap::new()));
        Ok(())
    }

    pub fn write(&mut self, path: &str, data: &[u8]) -> Result<(), Error> {
        let components = Self::parse(path)?;
        if components.is_empty() {
            return Err(Error::IsDir("/".to_owned()));
        }

        let parent = Self::parent_mut(&mut self.root, &components)?;
        let name = components[components.len() - 1];
        match parent.get_mut(name) {
            Some(Node::Directory(_)) => Err(Error::IsDir(Self::path_for(&components))),
            Some(Node::RegularFile(contents)) => {
                *contents = data.to_vec();
                Ok(())
            }
            None => {
                parent.insert(name.to_owned(), Node::RegularFile(data.to_vec()));
                Ok(())
            }
        }
    }

    pub fn read(&self, path: &str) -> Result<Vec<u8>, Error> {
        let components = Self::parse(path)?;
        match Self::lookup(&self.root, &components)? {
            Node::Directory(_) => Err(Error::IsDir(Self::path_for(&components))),
            Node::RegularFile(bytes) => Ok(bytes.clone()),
        }
    }

    pub fn ls(&self, path: &str) -> Result<Vec<(String, Kind)>, Error> {
        let components = Self::parse(path)?;
        match Self::lookup(&self.root, &components)? {
            Node::RegularFile(_) => Err(Error::NotDir(Self::path_for(&components))),
            Node::Directory(children) => Ok(children
                .iter()
                .map(|(name, node)| {
                    let kind = match node {
                        Node::Directory(_) => Kind::Dir,
                        Node::RegularFile(_) => Kind::File,
                    };
                    (name.clone(), kind)
                })
                .collect()),
        }
    }
}
