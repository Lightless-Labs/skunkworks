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

#[derive(Clone)]
struct Route {
    id: usize,
    method: String,
    segments: Vec<Segment>,
    trailing_slash: bool,
    rank: Vec<u8>,
    has_wildcard: bool,
}

pub struct Router {
    policy: SlashPolicy,
    routes: Vec<Route>,
}

fn valid_method(method: &str) -> bool {
    !method.is_empty() && method.bytes().all(|b| b.is_ascii_uppercase())
}

fn valid_name(name: &str) -> bool {
    let mut bytes = name.bytes();
    matches!(bytes.next(), Some(b'a'..=b'z'))
        && bytes.all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'_')
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
    let mut out = Vec::new();
    let mut i = 0;
    while i < bytes.len() {
        let b = bytes[i];
        if b == b'%' {
            if i + 2 >= bytes.len() {
                return Err(base + i);
            }
            let hi = hex_upper(bytes[i + 1]).ok_or(base + i)?;
            let lo = hex_upper(bytes[i + 2]).ok_or(base + i)?;
            let decoded = (hi << 4) | lo;
            if !(0x20..=0x7e).contains(&decoded)
                || decoded == b'/'
                || unreserved(decoded)
            {
                return Err(base + i);
            }
            out.push(decoded);
            i += 3;
        } else if unreserved(b) {
            out.push(b);
            i += 1;
        } else {
            return Err(base + i);
        }
    }
    Ok(String::from_utf8(out).expect("decoded segment is ASCII"))
}

fn path_parts<'a>(path: &'a str) -> Result<(Vec<(&'a str, usize)>, bool), usize> {
    if !path.starts_with('/') {
        return Err(0);
    }
    if path == "/" {
        return Ok((Vec::new(), false));
    }

    let bytes = path.as_bytes();
    let trailing = bytes.last() == Some(&b'/');
    let end = if trailing { path.len() - 1 } else { path.len() };
    let mut parts = Vec::new();
    let mut start = 1;
    let mut i = 1;
    while i <= end {
        if i == end || bytes[i] == b'/' {
            if i == start {
                return Err(start);
            }
            parts.push((&path[start..i], start));
            start = i + 1;
        }
        i += 1;
    }
    Ok((parts, trailing))
}

fn parse_pattern(pattern: &str) -> Result<(Vec<Segment>, bool, Vec<u8>, bool), usize> {
    let (parts, trailing) = path_parts(pattern)?;
    let mut segments = Vec::with_capacity(parts.len());
    let mut names = std::collections::HashSet::new();
    let mut rank = Vec::new();
    let mut has_wildcard = false;

    for (index, (raw, offset)) in parts.iter().enumerate() {
        if raw.starts_with(':') || raw.starts_with('*') {
            let marker = raw.as_bytes()[0];
            let name = &raw[1..];
            if !valid_name(name) || !names.insert(name.to_owned()) {
                return Err(*offset);
            }
            if marker == b'*' {
                if index + 1 != parts.len() {
                    return Err(*offset);
                }
                has_wildcard = true;
                segments.push(Segment::Wildcard(name.to_owned()));
                rank.push(0);
            } else {
                segments.push(Segment::Param(name.to_owned()));
                rank.push(1);
            }
        } else {
            let decoded = decode_segment(raw, *offset)?;
            segments.push(Segment::Static(decoded));
            rank.push(2);
        }
    }

    if !has_wildcard {
        rank.push(3);
    }
    Ok((segments, trailing, rank, has_wildcard))
}

fn parse_request(path: &str) -> Result<(Vec<String>, bool), usize> {
    let head_end = path
        .char_indices()
        .find(|(_, c)| *c == '?' || *c == '#')
        .map(|(i, _)| i)
        .unwrap_or(path.len());
    let head = &path[..head_end];
    let (parts, trailing) = path_parts(head)?;
    let mut decoded = Vec::with_capacity(parts.len());
    for (raw, offset) in parts {
        decoded.push(decode_segment(raw, offset)?);
    }
    Ok((decoded, trailing))
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
        let (segments, trailing_slash, rank, has_wildcard) =
            parse_pattern(pattern).map_err(|at| Error::InvalidPattern { at })?;
        let id = self.routes.len();
        self.routes.push(Route {
            id,
            method: method.to_owned(),
            segments,
            trailing_slash,
            rank,
            has_wildcard,
        });
        Ok(id)
    }

    pub fn resolve(&self, method: &str, path: &str) -> Result<Match, Error> {
        if !valid_method(method) {
            return Err(Error::InvalidMethod);
        }
        let (request, request_trailing) =
            parse_request(path).map_err(|at| Error::InvalidPath { at })?;

        let mut candidates: Vec<(&Route, Vec<(String, String)>)> = Vec::new();
        for route in &self.routes {
            if let Some(params) = match_route(route, &request) {
                if self.policy != SlashPolicy::Strict
                    || route.trailing_slash == request_trailing
                {
                    candidates.push((route, params));
                }
            }
        }

        if candidates.is_empty() {
            return Err(Error::NotFound);
        }

        let allowed = {
            let mut methods: Vec<String> = candidates
                .iter()
                .map(|(route, _)| route.method.clone())
                .collect();
            if methods.iter().any(|m| m == "GET") {
                methods.push("HEAD".to_owned());
            }
            methods.sort();
            methods.dedup();
            methods
        };

        let eligible: Vec<_> = if method == "HEAD" {
            if candidates.iter().any(|(route, _)| route.method == "HEAD") {
                candidates
                    .iter()
                    .filter(|(route, _)| route.method == "HEAD")
                    .collect()
            } else {
                candidates
                    .iter()
                    .filter(|(route, _)| route.method == "GET")
                    .collect()
            }
        } else {
            candidates
                .iter()
                .filter(|(route, _)| route.method == method)
                .collect()
        };

        if eligible.is_empty() {
            return Err(Error::MethodNotAllowed { allowed });
        }

        let (route, params) = eligible
            .into_iter()
            .max_by(|(a, _), (b, _)| a.rank.cmp(&b.rank).then_with(|| b.id.cmp(&a.id)))
            .expect("eligible candidates are nonempty");

        let redirect = if self.policy == SlashPolicy::Redirect
            && !request.is_empty()
            && route.trailing_slash != request_trailing
        {
            let head_end = path
                .char_indices()
                .find(|(_, c)| *c == '?' || *c == '#')
                .map(|(i, _)| i)
                .unwrap_or(path.len());
            let head = &path[..head_end];
            if route.trailing_slash {
                Some(format!("{head}/"))
            } else {
                Some(head[..head.len() - 1].to_owned())
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

fn match_route(route: &Route, request: &[String]) -> Option<Vec<(String, String)>> {
    let mut params = Vec::new();
    let mut i = 0;
    for segment in &route.segments {
        match segment {
            Segment::Static(expected) => {
                if request.get(i)? != expected {
                    return None;
                }
                i += 1;
            }
            Segment::Param(name) => {
                params.push((name.clone(), request.get(i)?.clone()));
                i += 1;
            }
            Segment::Wildcard(name) => {
                let value = request[i..].join("/");
                params.push((name.clone(), value));
                i = request.len();
            }
        }
    }
    if i == request.len() {
        Some(params)
    } else {
        None
    }
}

fn main() {}