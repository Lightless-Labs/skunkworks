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

#[derive(Debug, Clone, PartialEq, Eq)]
enum Entry {
    Dir(BTreeMap<String, Entry>),
    File(Vec<u8>),
}

pub struct Fs {
    root: Entry,
}

impl Fs {
    pub fn new() -> Self {
        Self {
            root: Entry::Dir(BTreeMap::new()),
        }
    }

    pub fn mkdir(&mut self, path: &str) -> Result<(), Error> {
        let parts = validate_path(path)?;
        if parts.is_empty() {
            return Err(Error::Exists("/".to_owned()));
        }

        let (parent_parts, name) = parts.split_at(parts.len() - 1);
        let parent_path = canonical_path(parent_parts);
        let final_path = canonical_path(&parts);

        let parent = self.lookup_dir(parent_parts, &parent_path)?;
        if parent.contains_key(&name[0]) {
            return Err(Error::Exists(final_path));
        }

        let parent = self.lookup_dir_mut(parent_parts, &parent_path)?;
        parent.insert(name[0].clone(), Entry::Dir(BTreeMap::new()));
        Ok(())
    }

    pub fn write(&mut self, path: &str, data: &[u8]) -> Result<(), Error> {
        let parts = validate_path(path)?;
        if parts.is_empty() {
            return Err(Error::IsDir("/".to_owned()));
        }

        let (parent_parts, name) = parts.split_at(parts.len() - 1);
        let parent_path = canonical_path(parent_parts);
        let final_path = canonical_path(&parts);

        let parent = self.lookup_dir(parent_parts, &parent_path)?;
        if matches!(parent.get(&name[0]), Some(Entry::Dir(_))) {
            return Err(Error::IsDir(final_path));
        }

        let parent = self.lookup_dir_mut(parent_parts, &parent_path)?;
        parent.insert(name[0].clone(), Entry::File(data.to_vec()));
        Ok(())
    }

    pub fn read(&self, path: &str) -> Result<Vec<u8>, Error> {
        let parts = validate_path(path)?;
        if parts.is_empty() {
            return Err(Error::IsDir("/".to_owned()));
        }

        let (parent_parts, name) = parts.split_at(parts.len() - 1);
        let parent_path = canonical_path(parent_parts);
        let final_path = canonical_path(&parts);
        let parent = self.lookup_dir(parent_parts, &parent_path)?;

        match parent.get(&name[0]) {
            Some(Entry::File(data)) => Ok(data.clone()),
            Some(Entry::Dir(_)) => Err(Error::IsDir(final_path)),
            None => Err(Error::Missing(final_path)),
        }
    }

    pub fn ls(&self, path: &str) -> Result<Vec<(String, Kind)>, Error> {
        let parts = validate_path(path)?;
        let canonical = canonical_path(&parts);
        let dir = self.lookup_dir(&parts, &canonical)?;

        Ok(dir
            .iter()
            .map(|(name, entry)| {
                let kind = match entry {
                    Entry::Dir(_) => Kind::Dir,
                    Entry::File(_) => Kind::File,
                };
                (name.clone(), kind)
            })
            .collect())
    }

    fn lookup_dir<'a>(
        &'a self,
        parts: &[String],
        canonical: &str,
    ) -> Result<&'a BTreeMap<String, Entry>, Error> {
        let mut current = match &self.root {
            Entry::Dir(children) => children,
            Entry::File(_) => unreachable!(),
        };
        let mut prefix = String::new();

        for component in parts {
            prefix.push('/');
            prefix.push_str(component);
            match current.get(component) {
                Some(Entry::Dir(children)) => current = children,
                Some(Entry::File(_)) => return Err(Error::NotDir(prefix)),
                None => return Err(Error::Missing(prefix)),
            }
        }

        let _ = canonical;
        Ok(current)
    }

    fn lookup_dir_mut<'a>(
        &'a mut self,
        parts: &[String],
        canonical: &str,
    ) -> Result<&'a mut BTreeMap<String, Entry>, Error> {
        let mut current = match &mut self.root {
            Entry::Dir(children) => children,
            Entry::File(_) => unreachable!(),
        };
        let mut prefix = String::new();

        for component in parts {
            prefix.push('/');
            prefix.push_str(component);
            match current.get_mut(component) {
                Some(Entry::Dir(children)) => current = children,
                Some(Entry::File(_)) => return Err(Error::NotDir(prefix)),
                None => return Err(Error::Missing(prefix)),
            }
        }

        let _ = canonical;
        Ok(current)
    }
}

impl Default for Fs {
    fn default() -> Self {
        Self::new()
    }
}

fn validate_path(path: &str) -> Result<Vec<String>, Error> {
    if let Some(index) = path.as_bytes().iter().position(|&byte| byte == 0) {
        return Err(Error::InvalidPath(index));
    }
    if path.is_empty() || !path.starts_with('/') {
        return Err(Error::InvalidPath(0));
    }
    if path == "/" {
        return Ok(Vec::new());
    }

    let bytes = path.as_bytes();
    let mut parts = Vec::new();
    let mut start = 1;

    for index in 1..=bytes.len() {
        if index == bytes.len() || bytes[index] == b'/' {
            if index == start {
                return Err(Error::InvalidPath(if index == bytes.len() {
                    bytes.len() - 1
                } else {
                    index
                }));
            }

            let component = &path[start..index];
            if component == "." || component == ".." {
                return Err(Error::InvalidPath(start));
            }
            parts.push(component.to_owned());
            start = index + 1;
        }
    }

    Ok(parts)
}

fn canonical_path(parts: &[String]) -> String {
    if parts.is_empty() {
        return "/".to_owned();
    }

    let mut path = String::new();
    for part in parts {
        path.push('/');
        path.push_str(part);
    }
    path
}

fn main() {}