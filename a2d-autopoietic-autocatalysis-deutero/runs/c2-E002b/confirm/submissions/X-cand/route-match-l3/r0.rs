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
enum Segment {
    Static(String),
    Param(String),
    Wildcard(String),
}

struct Route {
    method: String,
    segments: Vec<Segment>,
    trailing: bool,
    id: usize,
    rank: Vec<u8>,
}

pub struct Router {
    policy: SlashPolicy,
    routes: Vec<Route>,
}

fn valid_method(s: &str) -> bool {
    !s.is_empty() && s.bytes().all(|b| b.is_ascii_uppercase())
}

fn hex(b: u8) -> Option<u8> {
    match b {
        b'0'..=b'9' => Some(b - b'0'),
        b'A'..=b'F' => Some(b - b'A' + 10),
        _ => None,
    }
}

fn unreserved(b: u8) -> bool {
    b.is_ascii_alphanumeric() || matches!(b, b'.' | b'_' | b'~' | b'-')
}

fn decode_segment(s: &str, base: usize) -> Result<String, usize> {
    let bytes = s.as_bytes();
    let mut out = Vec::new();
    let mut i = 0;
    while i < bytes.len() {
        let b = bytes[i];
        if b == b'%' {
            if i + 2 >= bytes.len() {
                return Err(base + i);
            }
            let hi = hex(bytes[i + 1]).ok_or(base + i)?;
            let lo = hex(bytes[i + 2]).ok_or(base + i)?;
            let d = hi * 16 + lo;
            if !(0x20..=0x7e).contains(&d) || d == b'/' || unreserved(d) {
                return Err(base + i);
            }
            out.push(d);
            i += 3;
        } else if unreserved(b) {
            out.push(b);
            i += 1;
        } else {
            return Err(base + i);
        }
    }
    Ok(String::from_utf8(out).unwrap())
}

fn valid_name(s: &str) -> bool {
    let b = s.as_bytes();
    !b.is_empty()
        && b[0].is_ascii_lowercase()
        && b[1..]
            .iter()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || *c == b'_')
}

fn parse_pattern(pattern: &str) -> Result<(Vec<Segment>, bool), usize> {
    if !pattern.starts_with('/') {
        return Err(0);
    }
    if pattern == "/" {
        return Ok((Vec::new(), false));
    }

    let mut segs = Vec::new();
    let mut names = std::collections::HashSet::new();
    let mut start = 1usize;
    let mut trailing = false;

    while start <= pattern.len() {
        let rest = &pattern[start..];
        if rest.is_empty() {
            trailing = true;
            break;
        }
        let end = rest.find('/').map(|i| start + i).unwrap_or(pattern.len());
        if end == start {
            return Err(start);
        }
        let raw = &pattern[start..end];
        if raw.starts_with(':') || raw.starts_with('*') {
            let wildcard = raw.starts_with('*');
            let name = &raw[1..];
            if !valid_name(name) || !names.insert(name.to_string()) {
                return Err(start);
            }
            if wildcard && end != pattern.len() {
                return Err(start);
            }
            segs.push(if wildcard {
                Segment::Wildcard(name.to_string())
            } else {
                Segment::Param(name.to_string())
            });
        } else {
            let decoded = decode_segment(raw, start)?;
            segs.push(Segment::Static(decoded));
        }
        if end == pattern.len() {
            break;
        }
        start = end + 1;
        if start == pattern.len() {
            trailing = true;
            break;
        }
    }
    Ok((segs, trailing))
}

fn parse_path(path: &str) -> Result<(Vec<String>, bool), usize> {
    if !path.starts_with('/') {
        return Err(0);
    }
    if path == "/" {
        return Ok((Vec::new(), false));
    }
    let mut segs = Vec::new();
    let mut start = 1usize;
    let mut trailing = false;
    while start <= path.len() {
        let rest = &path[start..];
        if rest.is_empty() {
            trailing = true;
            break;
        }
        let end = rest.find('/').map(|i| start + i).unwrap_or(path.len());
        if end == start {
            return Err(start);
        }
        segs.push(decode_segment(&path[start..end], start)?);
        if end == path.len() {
            break;
        }
        start = end + 1;
        if start == path.len() {
            trailing = true;
            break;
        }
    }
    Ok((segs, trailing))
}

fn match_route(route: &Route, path: &[String]) -> Option<Vec<(String, String)>> {
    let mut params = Vec::new();
    let mut i = 0;
    for seg in &route.segments {
        match seg {
            Segment::Static(s) => {
                if path.get(i)? != s {
                    return None;
                }
                i += 1;
            }
            Segment::Param(name) => {
                params.push((name.clone(), path.get(i)?.clone()));
                i += 1;
            }
            Segment::Wildcard(name) => {
                params.push((name.clone(), path[i..].join("/")));
                i = path.len();
                break;
            }
        }
    }
    if i == path.len() {
        Some(params)
    } else {
        None
    }
}

impl Router {
    pub fn new(policy: SlashPolicy) -> Self {
        Self { policy, routes: Vec::new() }
    }

    pub fn add(&mut self, method: &str, pattern: &str) -> Result<usize, Error> {
        if !valid_method(method) {
            return Err(Error::InvalidMethod);
        }
        let (segments, trailing) =
            parse_pattern(pattern).map_err(|at| Error::InvalidPattern { at })?;
        let mut rank = Vec::new();
        let mut has_wildcard = false;
        for seg in &segments {
            match seg {
                Segment::Static(_) => rank.push(2),
                Segment::Param(_) => rank.push(1),
                Segment::Wildcard(_) => {
                    rank.push(0);
                    has_wildcard = true;
                }
            }
        }
        if !has_wildcard {
            rank.push(3);
        }
        let id = self.routes.len();
        self.routes.push(Route {
            method: method.to_string(),
            segments,
            trailing,
            id,
            rank,
        });
        Ok(id)
    }

    pub fn resolve(&self, method: &str, path: &str) -> Result<Match, Error> {
        if !valid_method(method) {
            return Err(Error::InvalidMethod);
        }
        let head_end = path
            .char_indices()
            .find(|(_, c)| *c == '?' || *c == '#')
            .map(|(i, _)| i)
            .unwrap_or(path.len());
        let head = &path[..head_end];
        let (segments, trailing) =
            parse_path(head).map_err(|at| Error::InvalidPath { at })?;

        let mut candidates: Vec<(&Route, Vec<(String, String)>)> = Vec::new();
        for route in &self.routes {
            if self.policy == SlashPolicy::Strict && route.trailing != trailing {
                continue;
            }
            if let Some(params) = match_route(route, &segments) {
                candidates.push((route, params));
            }
        }
        if candidates.is_empty() {
            return Err(Error::NotFound);
        }

        let eligible: Vec<usize> = if method == "HEAD" {
            let has_head = candidates.iter().any(|(r, _)| r.method == "HEAD");
            candidates
                .iter()
                .enumerate()
                .filter_map(|(i, (r, _))| {
                    (r.method == if has_head { "HEAD" } else { "GET" }).then_some(i)
                })
                .collect()
        } else {
            candidates
                .iter()
                .enumerate()
                .filter_map(|(i, (r, _))| (r.method == method).then_some(i))
                .collect()
        };

        if eligible.is_empty() {
            let mut allowed: Vec<String> =
                candidates.iter().map(|(r, _)| r.method.clone()).collect();
            if allowed.iter().any(|m| m == "GET") {
                allowed.push("HEAD".to_string());
            }
            allowed.sort();
            allowed.dedup();
            return Err(Error::MethodNotAllowed { allowed });
        }

        let winner = *eligible
            .iter()
            .max_by(|&&a, &&b| {
                candidates[a]
                    .0
                    .rank
                    .cmp(&candidates[b].0.rank)
                    .then_with(|| candidates[b].0.id.cmp(&candidates[a].0.id))
            })
            .unwrap();
        let (route, params) = &candidates[winner];
        let redirect = if self.policy == SlashPolicy::Redirect
            && !head.is_empty()
            && head != "/"
            && route.trailing != trailing
        {
            if route.trailing {
                Some(format!("{head}/"))
            } else {
                Some(head.strip_suffix('/').unwrap_or(head).to_string())
            }
        } else {
            None
        };

        Ok(Match {
            route: route.id,
            params: params.clone(),
            redirect,
        })
    }
}