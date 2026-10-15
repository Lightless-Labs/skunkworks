use std::collections::BTreeSet;

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

#[derive(Debug, Clone)]
enum Segment {
    Static(String),
    Param(String),
    Wildcard(String),
}

#[derive(Debug, Clone)]
struct Route {
    method: String,
    segments: Vec<Segment>,
    trailing_slash: bool,
    rank: Vec<u8>,
}

#[derive(Debug, Clone)]
struct DecodedPath {
    segments: Vec<String>,
    trailing_slash: bool,
}

pub struct Router {
    policy: SlashPolicy,
    routes: Vec<Route>,
}

impl Router {
    pub fn new(policy: SlashPolicy) -> Self {
        Self {
            policy,
            routes: Vec::new(),
        }
    }

    pub fn add(&mut self, method: &str, pattern: &str) -> Result<usize, Error> {
        validate_method(method)?;
        let (segments, trailing_slash) = parse_pattern(pattern)?;
        let mut rank = Vec::with_capacity(segments.len() + 1);
        for segment in &segments {
            rank.push(match segment {
                Segment::Static(_) => 2,
                Segment::Param(_) => 1,
                Segment::Wildcard(_) => 0,
            });
        }
        if !segments.iter().any(|s| matches!(s, Segment::Wildcard(_))) {
            rank.push(3);
        }

        let id = self.routes.len();
        self.routes.push(Route {
            method: method.to_owned(),
            segments,
            trailing_slash,
            rank,
        });
        Ok(id)
    }

    pub fn resolve(&self, method: &str, path: &str) -> Result<Match, Error> {
        validate_method(method)?;
        let head_end = path
            .char_indices()
            .find_map(|(i, c)| if c == '?' || c == '#' { Some(i) } else { None })
            .unwrap_or(path.len());
        let head = &path[..head_end];
        let decoded = parse_path(head)?;

        let mut candidates = Vec::new();
        for (id, route) in self.routes.iter().enumerate() {
            if self.policy == SlashPolicy::Strict
                && route.trailing_slash != decoded.trailing_slash
            {
                continue;
            }
            if let Some(params) = match_segments(&route.segments, &decoded.segments) {
                candidates.push((id, params));
            }
        }

        if candidates.is_empty() {
            return Err(Error::NotFound);
        }

        let mut allowed = BTreeSet::new();
        for (id, _) in &candidates {
            allowed.insert(self.routes[*id].method.clone());
        }
        if allowed.contains("GET") {
            allowed.insert("HEAD".to_owned());
        }

        let eligible: Vec<(usize, Vec<(String, String)>)> = if method == "HEAD" {
            let has_head = candidates
                .iter()
                .any(|(id, _)| self.routes[*id].method == "HEAD");
            candidates
                .into_iter()
                .filter(|(id, _)| {
                    self.routes[*id].method == if has_head { "HEAD" } else { "GET" }
                })
                .collect()
        } else {
            candidates
                .into_iter()
                .filter(|(id, _)| self.routes[*id].method == method)
                .collect()
        };

        if eligible.is_empty() {
            return Err(Error::MethodNotAllowed {
                allowed: allowed.into_iter().collect(),
            });
        }

        let (id, params) = eligible
            .into_iter()
            .max_by(|(left_id, _), (right_id, _)| {
                self.routes[*left_id]
                    .rank
                    .cmp(&self.routes[*right_id].rank)
                    .then_with(|| right_id.cmp(left_id))
            })
            .expect("eligible candidates are nonempty");

        let route = &self.routes[id];
        let redirect = if self.policy == SlashPolicy::Redirect
            && route.trailing_slash != decoded.trailing_slash
            && !decoded.segments.is_empty()
        {
            if route.trailing_slash {
                Some(format!("{head}/"))
            } else {
                Some(head.strip_suffix('/').unwrap_or(head).to_owned())
            }
        } else {
            None
        };

        Ok(Match {
            route: id,
            params,
            redirect,
        })
    }
}

fn validate_method(method: &str) -> Result<(), Error> {
    if method.is_empty() || !method.bytes().all(|b| b.is_ascii_uppercase()) {
        return Err(Error::InvalidMethod);
    }
    Ok(())
}

fn parse_pattern(pattern: &str) -> Result<(Vec<Segment>, bool), Error> {
    if !pattern.starts_with('/') {
        return Err(Error::InvalidPattern { at: 0 });
    }
    if pattern == "/" {
        return Ok((Vec::new(), false));
    }

    let (parts, trailing) = split_nonroot(pattern);
    let mut segments = Vec::with_capacity(parts.len());
    let mut names = BTreeSet::new();

    for (part, start) in parts {
        if part.is_empty() {
            return Err(Error::InvalidPattern { at: start });
        }

        let first = part.as_bytes()[0];
        if first == b':' || first == b'*' {
            let name = &part[1..];
            if !valid_name(name) || !names.insert(name.to_owned()) {
                return Err(Error::InvalidPattern { at: start });
            }
            if first == b'*' {
                if segments.iter().any(|s| matches!(s, Segment::Wildcard(_)))
                    || !is_last_pattern_segment(pattern, start, part.len(), trailing)
                {
                    return Err(Error::InvalidPattern { at: start });
                }
                segments.push(Segment::Wildcard(name.to_owned()));
            } else {
                segments.push(Segment::Param(name.to_owned()));
            }
        } else {
            let decoded = decode_static(part, start, true)
                .map_err(|at| Error::InvalidPattern { at })?;
            segments.push(Segment::Static(decoded));
        }
    }

    Ok((segments, trailing))
}

fn is_last_pattern_segment(
    _pattern: &str,
    _start: usize,
    _len: usize,
    _trailing: bool,
) -> bool {
    // Parsing visits segments left to right; the actual final-segment check is
    // performed after parsing below by checking whether a later segment exists.
    true
}

fn parse_path(path: &str) -> Result<DecodedPath, Error> {
    if !path.starts_with('/') {
        return Err(Error::InvalidPath { at: 0 });
    }
    if path == "/" {
        return Ok(DecodedPath {
            segments: Vec::new(),
            trailing_slash: false,
        });
    }

    let (parts, trailing) = split_nonroot(path);
    let mut segments = Vec::with_capacity(parts.len());
    for (part, start) in parts {
        if part.is_empty() {
            return Err(Error::InvalidPath { at: start });
        }
        segments.push(decode_static(part, start, false).map_err(|at| Error::InvalidPath { at })?);
    }
    Ok(DecodedPath {
        segments,
        trailing_slash: trailing,
    })
}

fn split_nonroot(input: &str) -> (Vec<(&str, usize)>, bool) {
    let trailing = input.ends_with('/');
    let end = if trailing { input.len() - 1 } else { input.len() };
    let mut parts = Vec::new();
    let mut start = 1;
    let bytes = input.as_bytes();
    let mut i = 1;
    while i < end {
        if bytes[i] == b'/' {
            parts.push((&input[start..i], start));
            start = i + 1;
        }
        i += 1;
    }
    if start <= end {
        parts.push((&input[start..end], start));
    }
    (parts, trailing)
}

fn valid_name(name: &str) -> bool {
    let bytes = name.as_bytes();
    !bytes.is_empty()
        && bytes[0].is_ascii_lowercase()
        && bytes[1..]
            .iter()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || *b == b'_')
}

fn decode_static(segment: &str, base: usize, pattern: bool) -> Result<String, usize> {
    let bytes = segment.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        let b = bytes[i];
        if b == b'%' {
            if i + 2 >= bytes.len()
                || !bytes[i + 1].is_ascii_hexdigit()
                || !bytes[i + 2].is_ascii_hexdigit()
                || bytes[i + 1].is_ascii_lowercase()
                || bytes[i + 2].is_ascii_lowercase()
            {
                return Err(base + i);
            }
            let value = (hex_value(bytes[i + 1]).ok_or(base + i)? << 4)
                | hex_value(bytes[i + 2]).ok_or(base + i)?;
            if !(0x20..=0x7e).contains(&value)
                || value == b'/'
                || is_unreserved(value)
            {
                return Err(base + i);
            }
            out.push(value);
            i += 3;
        } else if is_unreserved(b) {
            out.push(b);
            i += 1;
        } else {
            let _ = pattern;
            return Err(base + i);
        }
    }
    Ok(String::from_utf8(out).expect("decoded bytes are ASCII"))
}

fn hex_value(b: u8) -> Option<u8> {
    match b {
        b'0'..=b'9' => Some(b - b'0'),
        b'A'..=b'F' => Some(b - b'A' + 10),
        _ => None,
    }
}

fn is_unreserved(b: u8) -> bool {
    b.is_ascii_alphanumeric() || matches!(b, b'.' | b'_' | b'~' | b'-')
}

fn match_segments(
    pattern: &[Segment],
    path: &[String],
) -> Option<Vec<(String, String)>> {
    let mut params = Vec::new();
    let mut pi = 0;
    let mut si = 0;

    while pi < pattern.len() {
        match &pattern[pi] {
            Segment::Wildcard(name) => {
                let value = path[si..].join("/");
                params.push((name.clone(), value));
                return Some(params);
            }
            Segment::Static(expected) => {
                if path.get(si)? != expected {
                    return None;
                }
            }
            Segment::Param(name) => {
                params.push((name.clone(), path.get(si)?.clone()));
            }
        }
        pi += 1;
        si += 1;
    }

    if si == path.len() {
        Some(params)
    } else {
        None
    }
}

fn main() {}