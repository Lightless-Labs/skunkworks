// Fault: accepts noncanonical escapes
use std::collections::BTreeSet;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Error {
    InvalidPattern { at: usize },
    InvalidPath { at: usize },
    NotFound,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Match {
    pub route: usize,
    pub params: Vec<(String, String)>,
}

#[derive(Clone)]
enum Part {
    Lit(String),
    Param(String),
    Wild(String),
}
struct Route {
    parts: Vec<Part>,
    trailing: bool,
}
pub struct Router {
    routes: Vec<Route>,
}
fn unreserved(b: u8) -> bool {
    b.is_ascii_alphanumeric() || b"._~-".contains(&b)
}
fn nibble(b: u8) -> Option<u8> {
    match b {
        b'0'..=b'9' => Some(b - b'0'),
        b'A'..=b'F' => Some(b - b'A' + 10),
        _ => None,
    }
}
fn decode(s: &str, base: usize) -> Result<String, usize> {
    let b = s.as_bytes();
    let mut out = String::new();
    let mut i = 0;
    while i < b.len() {
        if unreserved(b[i]) {
            out.push(b[i] as char);
            i += 1;
        } else if b[i] == b'%' {
            let hi = b.get(i + 1).and_then(|v| nibble(*v));
            let lo = b.get(i + 2).and_then(|v| nibble(*v));
            let v = match (hi, lo) {
                (Some(h), Some(l)) => h * 16 + l,
                _ => return Err(base + i),
            };
            if !(32..=126).contains(&v) || v == b'/' {
                return Err(base + i);
            }
            out.push(v as char);
            i += 3;
        } else {
            return Err(base + i);
        }
    }
    Ok(out)
}
fn capture_name(s: &str) -> bool {
    !s.is_empty()
        && s.as_bytes()[0].is_ascii_lowercase()
        && s.bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'_')
}
fn parse(s: &str, pattern: bool, allow_trailing: bool) -> Result<(Vec<Part>, bool), usize> {
    if !s.starts_with('/') {
        return Err(0);
    }
    if s == "/" {
        return Ok((Vec::new(), false));
    }
    let trailing = s.ends_with('/');
    let end = if trailing { s.len() - 1 } else { s.len() };
    let mut parts = Vec::new();
    let mut seen = BTreeSet::new();
    let mut start = 1;
    // Scan in byte order, including interior empty segments, before a trailing-slash error.
    for seg in s[1..end].split('/') {
        if seg.is_empty() {
            return Err(start);
        }
        let part = if pattern && (seg.starts_with(':') || seg.starts_with('*')) {
            let wild = seg.starts_with('*');
            if wild {
                return Err(start);
            }
            if !capture_name(&seg[1..]) || !seen.insert(seg[1..].to_string()) {
                return Err(start);
            }
            if wild && start + seg.len() != end {
                return Err(start);
            }
            if wild {
                Part::Wild(seg[1..].to_string())
            } else {
                Part::Param(seg[1..].to_string())
            }
        } else {
            Part::Lit(decode(seg, start)?)
        };
        parts.push(part);
        start += seg.len() + 1;
    }
    if trailing && !allow_trailing {
        return Err(s.len() - 1);
    }
    Ok((parts, trailing))
}
fn request_path(s: &str, allow_trailing: bool) -> Result<(Vec<String>, bool, String), Error> {
    let head = s.split(['?', '#']).next().unwrap();
    let (parts, trailing) = parse(head, false, allow_trailing).map_err(|at| Error::InvalidPath { at })?;
    let values = parts
        .into_iter()
        .map(|p| match p {
            Part::Lit(v) => v,
            _ => unreachable!(),
        })
        .collect();
    Ok((values, trailing, head.to_string()))
}
fn fit(parts: &[Part], values: &[String]) -> Option<(Vec<u8>, Vec<(String, String)>)> {
    let mut rank = Vec::new();
    let mut captures = Vec::new();
    let mut pos = 0;
    for p in parts {
        match p {
            Part::Wild(n) => {
                rank.push(0);
                captures.push((n.clone(), values[pos..].join("/")));
                return Some((rank, captures));
            }
            Part::Lit(lit) => {
                if values.get(pos) != Some(lit) {
                    return None;
                }
                rank.push(2);
            }
            Part::Param(n) => {
                captures.push((n.clone(), values.get(pos)?.clone()));
                rank.push(1);
            }
        }
        pos += 1;
    }
    if pos != values.len() {
        return None;
    }
    rank.push(3);
    Some((rank, captures))
}

impl Router {
    pub fn new() -> Self {
        Self { routes: Vec::new() }
    }
    pub fn add(&mut self, pattern: &str) -> Result<usize, Error> {
        let (parts, trailing) = parse(pattern, true, false).map_err(|at| Error::InvalidPattern { at })?;
        let id = self.routes.len();
        self.routes.push(Route { parts, trailing });
        Ok(id)
    }
    pub fn resolve(&self, path: &str) -> Result<Match, Error> {
        let (values, trailing, head) = request_path(path, false)?;
        let mut candidates = Vec::new();
        for (id, r) in self.routes.iter().enumerate() {
            if let Some((rank, params)) = fit(&r.parts, &values) {
                candidates.push((id, rank, params));
            }
        }
        let mut best: Option<(usize, Vec<u8>, Vec<(String, String)>)> = None;
        for candidate in candidates {
            if best.as_ref().is_none_or(|old| candidate.1 > old.1) {
                best = Some(candidate);
            }
        }
        let (route, _, params) = best.ok_or(Error::NotFound)?;
        Ok(Match { route, params })
    }
}
