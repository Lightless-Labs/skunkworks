// Fault: extra parameter checked before missing
use std::collections::BTreeSet;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Error {
    InvalidPattern { at: usize },
    InvalidPath { at: usize },
    NotFound,
    InvalidMethod,
    MethodNotAllowed { allowed: Vec<String> },
    InvalidName,
    DuplicateName,
    Conflict { with: usize },
    UnknownName,
    DuplicateParam { name: String },
    MissingParam { name: String },
    ExtraParam { name: String },
    InvalidValue { name: String },
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
    Lit(String),
    Param(String),
    Wild(String),
}
struct Route {
    parts: Vec<Part>,
    trailing: bool,
    method: String,
    name: String,
}
pub struct Router {
    routes: Vec<Route>,
    policy: SlashPolicy,
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
            if !(32..=126).contains(&v) || v == b'/' || unreserved(v) {
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
fn valid_method(s: &str) -> bool {
    !s.is_empty() && s.bytes().all(|b| b.is_ascii_uppercase())
}

fn route_name(s: &str) -> bool {
    !s.is_empty()
        && s.as_bytes()[0].is_ascii_lowercase()
        && s.bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b"_-".contains(&b))
}
fn same_shape(a: &[Part], b: &[Part]) -> bool {
    a.len() == b.len()
        && a.iter().zip(b).all(|(x, y)| match (x, y) {
            (Part::Lit(s), Part::Lit(t)) => s == t,
            (Part::Param(_), Part::Param(_)) | (Part::Wild(_), Part::Wild(_)) => true,
            _ => false,
        })
}
fn encode(s: &str) -> String {
    let mut out = String::new();
    for b in s.bytes() {
        if unreserved(b) {
            out.push(b as char);
        } else {
            out.push('%');
            out.push(b"0123456789ABCDEF"[(b >> 4) as usize] as char);
            out.push(b"0123456789ABCDEF"[(b & 15) as usize] as char);
        }
    }
    out
}

impl Router {
    pub fn new(policy: SlashPolicy) -> Self {
        Self {
            routes: Vec::new(),
            policy,
        }
    }
    pub fn add(&mut self, name: &str, method: &str, pattern: &str) -> Result<usize, Error> {
        if !valid_method(method) {
            return Err(Error::InvalidMethod);
        }
        if !route_name(name) {
            return Err(Error::InvalidName);
        }
        let (parts, trailing) = parse(pattern, true, true).map_err(|at| Error::InvalidPattern { at })?;
        if self.routes.iter().any(|r| r.name == name) {
            return Err(Error::DuplicateName);
        }
        for (id, r) in self.routes.iter().enumerate() {
            if r.method == method
                && (self.policy != SlashPolicy::Strict || r.trailing == trailing)
                && same_shape(&r.parts, &parts)
            {
                return Err(Error::Conflict { with: id });
            }
        }
        let id = self.routes.len();
        self.routes.push(Route {
            parts,
            trailing,
            method: method.to_string(),
            name: name.to_string(),
        });
        Ok(id)
    }
    pub fn resolve(&self, method: &str, path: &str) -> Result<Match, Error> {
        if !valid_method(method) {
            return Err(Error::InvalidMethod);
        }
        let (values, trailing, head) = request_path(path, true)?;
        let mut candidates = Vec::new();
        for (id, r) in self.routes.iter().enumerate() {
            if self.policy == SlashPolicy::Strict && r.trailing != trailing {
                continue;
            }
            if let Some((rank, params)) = fit(&r.parts, &values) {
                candidates.push((id, rank, params));
            }
        }
        if candidates.is_empty() {
            return Err(Error::NotFound);
        }
        let effective = if method == "HEAD" && !candidates.iter().any(|c| self.routes[c.0].method == "HEAD") {
            "GET"
        } else {
            method
        };
        let mut allowed = BTreeSet::new();
        for c in &candidates {
            let m = &self.routes[c.0].method;
            allowed.insert(m.clone());
            if m == "GET" {
                allowed.insert("HEAD".to_string());
            }
        }
        candidates.retain(|c| self.routes[c.0].method == effective);
        if candidates.is_empty() {
            return Err(Error::MethodNotAllowed {
                allowed: allowed.into_iter().collect(),
            });
        }
        let mut best: Option<(usize, Vec<u8>, Vec<(String, String)>)> = None;
        for candidate in candidates {
            if best.as_ref().is_none_or(|old| candidate.1 > old.1) {
                best = Some(candidate);
            }
        }
        let (route, _, params) = best.ok_or(Error::NotFound)?;
        let redirect =
            if self.policy == SlashPolicy::Redirect && head != "/" && self.routes[route].trailing != trailing
            {
                Some(if self.routes[route].trailing {
                    format!("{head}/")
                } else {
                    head.trim_end_matches('/').to_string()
                })
            } else {
                None
            };
        Ok(Match {
            route,
            params,
            redirect,
        })
    }

    pub fn build(&self, name: &str, params: &[(&str, &str)]) -> Result<String, Error> {
        let r = self
            .routes
            .iter()
            .find(|r| r.name == name)
            .ok_or(Error::UnknownName)?;
        let mut keys = BTreeSet::new();
        for (key, _) in params {
            if !keys.insert(*key) {
                return Err(Error::DuplicateParam {
                    name: (*key).to_string(),
                });
            }
        }
        let required: Vec<&str> = r
            .parts
            .iter()
            .filter_map(|p| match p {
                Part::Param(n) | Part::Wild(n) => Some(n.as_str()),
                _ => None,
            })
            .collect();
        if let Some(n) = keys.iter().find(|n| !required.contains(n)) {
            return Err(Error::ExtraParam {
                name: (*n).to_string(),
            });
        }
        for n in &required {
            if !keys.contains(n) {
                return Err(Error::MissingParam {
                    name: (*n).to_string(),
                });
            }
        }
        for p in &r.parts {
            if let Part::Param(n) | Part::Wild(n) = p {
                let v = params.iter().find(|(k, _)| *k == n).unwrap().1;
                let valid_ascii = v.bytes().all(|b| (32..=126).contains(&b));
                let valid = match p {
                    Part::Param(_) => !v.is_empty() && !v.contains('/'),
                    Part::Wild(_) => v.is_empty() || v.split('/').all(|s| !s.is_empty()),
                    _ => unreachable!(),
                };
                if !valid_ascii || !valid {
                    return Err(Error::InvalidValue { name: n.clone() });
                }
            }
        }
        let mut segments = Vec::new();
        for p in &r.parts {
            match p {
                Part::Lit(s) => segments.push(encode(s)),
                Part::Param(n) => segments.push(encode(params.iter().find(|(k, _)| *k == n).unwrap().1)),
                Part::Wild(n) => {
                    let v = params.iter().find(|(k, _)| *k == n).unwrap().1;
                    if !v.is_empty() {
                        segments.extend(v.split('/').map(encode));
                    }
                }
            }
        }
        let mut out = format!("/{}", segments.join("/"));
        if r.trailing && out != "/" {
            out.push('/');
        }
        Ok(out)
    }
}
