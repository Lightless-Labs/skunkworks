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
    trailing_slash: bool,
    id: usize,
    rank: Vec<u8>,
}

pub struct Router {
    policy: SlashPolicy,
    routes: Vec<Route>,
}

fn valid_method(method: &str) -> bool {
    !method.is_empty() && method.bytes().all(|b| b.is_ascii_uppercase())
}

fn unreserved(b: u8) -> bool {
    b.is_ascii_alphanumeric() || matches!(b, b'.' | b'_' | b'~' | b'-')
}

fn hex_upper(b: u8) -> Option<u8> {
    match b {
        b'0'..=b'9' => Some(b - b'0'),
        b'A'..=b'F' => Some(b - b'A' + 10),
        _ => None,
    }
}

fn decode_segment(raw: &str, base: usize) -> Result<String, usize> {
    let bytes = raw.as_bytes();
    let mut decoded = Vec::with_capacity(bytes.len());
    let mut i = 0;

    while i < bytes.len() {
        let b = bytes[i];
        if b == b'%' {
            if i + 2 >= bytes.len() {
                return Err(base + i);
            }
            let hi = hex_upper(bytes[i + 1]).ok_or(base + i)?;
            let lo = hex_upper(bytes[i + 2]).ok_or(base + i)?;
            let value = (hi << 4) | lo;
            if !(0x20..=0x7e).contains(&value) || value == b'/' || unreserved(value) {
                return Err(base + i);
            }
            decoded.push(value);
            i += 3;
        } else if unreserved(b) {
            decoded.push(b);
            i += 1;
        } else {
            return Err(base + i);
        }
    }

    // All decoded bytes are printable ASCII.
    Ok(String::from_utf8(decoded).expect("decoded bytes are ASCII"))
}

fn valid_name(name: &str) -> bool {
    let bytes = name.as_bytes();
    !bytes.is_empty()
        && bytes[0].is_ascii_lowercase()
        && bytes[1..]
            .iter()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || *b == b'_')
}

fn parse_pattern(pattern: &str) -> Result<(Vec<Segment>, bool, Vec<u8>), Error> {
    if !pattern.starts_with('/') {
        return Err(Error::InvalidPattern { at: 0 });
    }
    if pattern == "/" {
        return Ok((Vec::new(), false, vec![3]));
    }

    let bytes = pattern.as_bytes();
    let mut pieces: Vec<(usize, &str)> = Vec::new();
    let mut start = 1;
    for i in 1..bytes.len() {
        if bytes[i] == b'/' {
            pieces.push((start, &pattern[start..i]));
            start = i + 1;
        }
    }

    let mut trailing = false;
    if start == bytes.len() {
        trailing = true;
    } else {
        pieces.push((start, &pattern[start..]));
    }

    let mut segments = Vec::new();
    let mut names = std::collections::HashSet::new();
    let mut rank = Vec::new();

    for (idx, (at, raw)) in pieces.iter().enumerate() {
        if raw.is_empty() {
            return Err(Error::InvalidPattern { at: *at });
        }
        let first = raw.as_bytes()[0];
        if first == b':' || first == b'*' {
            let name = &raw[1..];
            if !valid_name(name) || !names.insert(name.to_string()) {
                return Err(Error::InvalidPattern { at: *at });
            }
            if first == b'*' {
                if idx + 1 != pieces.len() {
                    return Err(Error::InvalidPattern { at: *at });
                }
                segments.push(Segment::Wildcard(name.to_string()));
                rank.push(0);
            } else {
                segments.push(Segment::Param(name.to_string()));
                rank.push(1);
            }
        } else {
            let value = decode_segment(raw, *at)
                .map_err(|at| Error::InvalidPattern { at })?;
            segments.push(Segment::Static(value));
            rank.push(2);
        }
    }

    if !matches!(segments.last(), Some(Segment::Wildcard(_))) {
        rank.push(3);
    }
    Ok((segments, trailing, rank))
}

fn parse_path(path: &str) -> Result<(Vec<String>, bool), Error> {
    let head_end = path
        .char_indices()
        .find_map(|(i, c)| if c == '?' || c == '#' { Some(i) } else { None })
        .unwrap_or(path.len());
    let head = &path[..head_end];

    if !head.starts_with('/') {
        return Err(Error::InvalidPath { at: 0 });
    }
    if head == "/" {
        return Ok((Vec::new(), false));
    }

    let bytes = head.as_bytes();
    let mut raw_segments: Vec<(usize, &str)> = Vec::new();
    let mut start = 1;
    for i in 1..bytes.len() {
        if bytes[i] == b'/' {
            raw_segments.push((start, &head[start..i]));
            start = i + 1;
        }
    }

    let mut trailing = false;
    if start == bytes.len() {
        trailing = true;
    } else {
        raw_segments.push((start, &head[start..]));
    }

    let mut segments = Vec::with_capacity(raw_segments.len());
    for (at, raw) in raw_segments {
        if raw.is_empty() {
            return Err(Error::InvalidPath { at });
        }
        segments.push(
            decode_segment(raw, at).map_err(|at| Error::InvalidPath { at })?,
        );
    }
    Ok((segments, trailing))
}

fn path_match(route: &Route, path: &[String]) -> Option<Vec<(String, String)>> {
    let wildcard_at = route
        .segments
        .iter()
        .position(|s| matches!(s, Segment::Wildcard(_)));
    let prefix_len = wildcard_at.unwrap_or(route.segments.len());

    if wildcard_at.is_none() && route.segments.len() != path.len() {
        return None;
    }
    if wildcard_at.is_some() && path.len() < prefix_len {
        return None;
    }

    let mut params = Vec::new();
    for i in 0..prefix_len {
        match &route.segments[i] {
            Segment::Static(value) if value != &path[i] => return None,
            Segment::Static(_) => {}
            Segment::Param(name) => params.push((name.clone(), path[i].clone())),
            Segment::Wildcard(_) => unreachable!(),
        }
    }

    if let Some(i) = wildcard_at {
        if let Segment::Wildcard(name) = &route.segments[i] {
            params.push((name.clone(), path[i..].join("/")));
        }
    }

    Some(params)
}

impl Router {
    pub fn new(policy: SlashPolicy) -> Self {
        Self {
            policy,
            routes: Vec::new(),
        }
    }

    pub fn add(&mut self, method: &str, pattern: &str) -> Result<usize, Error> {
        if !valid_method(method) {
            return Err(Error::InvalidMethod);
        }
        let (segments, trailing_slash, rank) = parse_pattern(pattern)?;
        let id = self.routes.len();
        self.routes.push(Route {
            method: method.to_string(),
            segments,
            trailing_slash,
            id,
            rank,
        });
        Ok(id)
    }

    pub fn resolve(&self, method: &str, path: &str) -> Result<Match, Error> {
        if !valid_method(method) {
            return Err(Error::InvalidMethod);
        }
        let (path_segments, path_trailing) = parse_path(path)?;

        let mut candidates: Vec<(&Route, Vec<(String, String)>)> = Vec::new();
        for route in &self.routes {
            if self.policy == SlashPolicy::Strict && route.trailing_slash != path_trailing {
                continue;
            }
            if let Some(params) = path_match(route, &path_segments) {
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
                    if (has_head && r.method == "HEAD") || (!has_head && r.method == "GET") {
                        Some(i)
                    } else {
                        None
                    }
                })
                .collect()
        } else {
            candidates
                .iter()
                .enumerate()
                .filter_map(|(i, (r, _))| if r.method == method { Some(i) } else { None })
                .collect()
        };

        if eligible.is_empty() {
            let mut allowed: Vec<String> = candidates
                .iter()
                .map(|(r, _)| r.method.clone())
                .collect();
            if allowed.iter().any(|m| m == "GET") {
                allowed.push("HEAD".to_string());
            }
            allowed.sort();
            allowed.dedup();
            return Err(Error::MethodNotAllowed { allowed });
        }

        let winner_idx = *eligible
            .iter()
            .max_by(|&&a, &&b| {
                candidates[a]
                    .0
                    .rank
                    .cmp(&candidates[b].0.rank)
                    .then_with(|| candidates[b].0.id.cmp(&candidates[a].0.id))
            })
            .expect("eligible candidates are nonempty");
        let (route, params) = &candidates[winner_idx];

        let redirect = if self.policy == SlashPolicy::Redirect
            && route.trailing_slash != path_trailing
            && !path_segments.is_empty()
        {
            let end = path
                .char_indices()
                .find_map(|(i, c)| if c == '?' || c == '#' { Some(i) } else { None })
                .unwrap_or(path.len());
            let mut head = path[..end].to_string();
            if route.trailing_slash {
                head.push('/');
            } else if head.ends_with('/') {
                head.pop();
            }
            Some(head)
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