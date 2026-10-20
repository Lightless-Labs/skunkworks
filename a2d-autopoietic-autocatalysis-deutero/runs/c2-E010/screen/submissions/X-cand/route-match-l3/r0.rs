#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Error {
    InvalidPattern { at: usize },
    InvalidPath { at: usize },
    NotFound,
    InvalidMethod,
    MethodNotAllowed { allowed: Vec<String> },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Match {
    pub route: usize,
    pub params: Vec<(String, String)>,
    pub redirect: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SlashPolicy {
    Strict,
    Ignore,
    Redirect,
}

#[derive(Clone)]
enum Part {
    Static(String),
    Param(String),
    Wildcard(String),
}

struct Entry {
    method: String,
    parts: Vec<Part>,
    trailing: bool,
    wildcard: bool,
}

pub struct Router {
    policy: SlashPolicy,
    entries: Vec<Entry>,
}

fn method_ok(s: &str) -> bool {
    !s.is_empty() && s.bytes().all(|b| b.is_ascii_uppercase())
}

fn is_unreserved(b: u8) -> bool {
    b.is_ascii_alphanumeric() || matches!(b, b'.' | b'_' | b'~' | b'-')
}

fn valid_name(s: &str) -> bool {
    let mut it = s.bytes();
    matches!(it.next(), Some(b'a'..=b'z'))
        && it.all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'_')
}

fn err(path: bool, at: usize) -> Error {
    if path {
        Error::InvalidPath { at }
    } else {
        Error::InvalidPattern { at }
    }
}

fn decode(raw: &str, base: usize, path: bool) -> Result<String, Error> {
    let b = raw.as_bytes();
    let mut out = Vec::new();
    let mut i = 0;
    while i < b.len() {
        if b[i] == b'%' {
            if i + 2 >= b.len() {
                return Err(err(path, base + i));
            }
            let h = |c: u8| match c {
                b'0'..=b'9' => Some(c - b'0'),
                b'A'..=b'F' => Some(c - b'A' + 10),
                _ => None,
            };
            let Some(v) = h(b[i + 1]).zip(h(b[i + 2])).map(|(a, z)| a * 16 + z) else {
                return Err(err(path, base + i));
            };
            if !(0x20..=0x7e).contains(&v) || v == b'/' || is_unreserved(v) {
                return Err(err(path, base + i));
            }
            out.push(v);
            i += 3;
        } else if is_unreserved(b[i]) {
            out.push(b[i]);
            i += 1;
        } else {
            return Err(err(path, base + i));
        }
    }
    Ok(String::from_utf8(out).unwrap())
}

fn split(input: &str, path: bool) -> Result<(Vec<(&str, usize)>, bool), Error> {
    if !input.starts_with('/') {
        return Err(err(path, 0));
    }
    if input == "/" {
        return Ok((Vec::new(), false));
    }
    let bytes = input.as_bytes();
    let mut end = bytes.len();
    let trailing = bytes[end - 1] == b'/';
    if trailing {
        end -= 1;
    }
    let mut out = Vec::new();
    let mut start = 1;
    while start <= end {
        let mut stop = start;
        while stop < end && bytes[stop] != b'/' {
            stop += 1;
        }
        if stop == start {
            return Err(err(path, start));
        }
        out.push((&input[start..stop], start));
        if stop == end {
            break;
        }
        start = stop + 1;
    }
    Ok((out, trailing))
}

impl Router {
    pub fn new(policy: SlashPolicy) -> Self {
        Self { policy, entries: Vec::new() }
    }

    pub fn add(&mut self, method: &str, pattern: &str) -> Result<usize, Error> {
        if !method_ok(method) {
            return Err(Error::InvalidMethod);
        }
        let (raw, trailing) = split(pattern, false)?;
        let mut parts = Vec::new();
        let mut names = std::collections::HashSet::new();
        let mut wildcard = false;
        for (i, (s, at)) in raw.iter().enumerate() {
            let b = s.as_bytes();
            if b[0] == b':' || b[0] == b'*' {
                let name = &s[1..];
                if !valid_name(name) || !names.insert(name.to_string()) {
                    return Err(Error::InvalidPattern { at: *at });
                }
                if b[0] == b'*' && i + 1 != raw.len() {
                    return Err(Error::InvalidPattern { at: *at });
                }
                if b[0] == b'*' {
                    wildcard = true;
                    parts.push(Part::Wildcard(name.to_string()));
                } else {
                    parts.push(Part::Param(name.to_string()));
                }
            } else {
                parts.push(Part::Static(decode(s, *at, false)?));
            }
        }
        let id = self.entries.len();
        self.entries.push(Entry { method: method.to_string(), parts, trailing, wildcard });
        Ok(id)
    }

    pub fn resolve(&self, method: &str, path: &str) -> Result<Match, Error> {
        if !method_ok(method) {
            return Err(Error::InvalidMethod);
        }
        let head_end = path.bytes().position(|b| b == b'?' || b == b'#').unwrap_or(path.len());
        let head = &path[..head_end];
        let (raw, trailing) = split(head, true)?;
        let mut decoded = Vec::new();
        for (s, at) in raw {
            decoded.push(decode(s, at, true)?);
        }

        let matches = |entry: &Entry| -> Option<Vec<(String, String)>> {
            if self.policy == SlashPolicy::Strict && entry.trailing != trailing {
                return None;
            }
            if entry.wildcard {
                if entry.parts.len() - 1 > decoded.len() {
                    return None;
                }
            } else if entry.parts.len() != decoded.len() {
                return None;
            }
            let mut params = Vec::new();
            for (i, part) in entry.parts.iter().enumerate() {
                match part {
                    Part::Static(v) if decoded.get(i) != Some(v) => return None,
                    Part::Static(_) => {}
                    Part::Param(n) => params.push((n.clone(), decoded[i].clone())),
                    Part::Wildcard(n) => params.push((n.clone(), decoded[i..].join("/"))),
                }
            }
            Some(params)
        };

        let mut all = Vec::new();
        for (id, entry) in self.entries.iter().enumerate() {
            if let Some(params) = matches(entry) {
                all.push((id, params));
            }
        }
        if all.is_empty() {
            return Err(Error::NotFound);
        }

        let has_head = method == "HEAD" && all.iter().any(|(id, _)| self.entries[*id].method == "HEAD");
        let wanted = if method == "HEAD" && !has_head { "GET" } else { method };
        let mut eligible: Vec<_> = all.iter().filter(|(id, _)| self.entries[*id].method == wanted).cloned().collect();

        if eligible.is_empty() {
            let mut allowed = std::collections::BTreeSet::new();
            for (id, _) in &all {
                allowed.insert(self.entries[*id].method.clone());
            }
            if allowed.contains("GET") {
                allowed.insert("HEAD".to_string());
            }
            return Err(Error::MethodNotAllowed { allowed: allowed.into_iter().collect() });
        }

        let rank = |entry: &Entry| {
            let mut r = Vec::new();
            for p in &entry.parts {
                r.push(match p { Part::Static(_) => 2, Part::Param(_) => 1, Part::Wildcard(_) => 0 });
            }
            if !entry.wildcard { r.push(3); }
            r
        };
        let mut best = eligible.remove(0);
        for candidate in eligible {
            if rank(&self.entries[candidate.0]) > rank(&self.entries[best.0]) {
                best = candidate;
            }
        }
        let entry = &self.entries[best.0];
        let redirect = if self.policy == SlashPolicy::Redirect && entry.trailing != trailing {
            let mut target = head.to_string();
            if trailing { target.pop(); } else if head != "/" { target.push('/'); }
            Some(target)
        } else { None };

        Ok(Match { route: best.0, params: best.1, redirect })
    }
}
