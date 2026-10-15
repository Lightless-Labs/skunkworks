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
    id: usize,
    method: String,
    segments: Vec<Segment>,
    trailing_slash: bool,
    rank: Vec<u8>,
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
        let mut has_wildcard = false;
        for segment in &segments {
            match segment {
                Segment::Static(_) => rank.push(2),
                Segment::Param(_) => rank.push(1),
                Segment::Wildcard(_) => {
                    rank.push(0);
                    has_wildcard = true;
                    break;
                }
            }
        }
        if !has_wildcard {
            rank.push(3);
        }

        let id = self.routes.len();
        self.routes.push(Route {
            id,
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
            .bytes()
            .position(|b| b == b'?' || b == b'#')
            .unwrap_or(path.len());
        let head = &path[..head_end];
        let (request_segments, request_trailing) = parse_path(head)?;

        let mut candidates: Vec<(&Route, Vec<(String, String)>)> = Vec::new();
        for route in &self.routes {
            if self.policy == SlashPolicy::Strict && route.trailing_slash != request_trailing {
                continue;
            }
            if let Some(params) = match_route(&route.segments, &request_segments) {
                candidates.push((route, params));
            }
        }

        if candidates.is_empty() {
            return Err(Error::NotFound);
        }

        let eligible: Vec<usize> = if method == "HEAD" {
            let has_head = candidates.iter().any(|(route, _)| route.method == "HEAD");
            candidates
                .iter()
                .enumerate()
                .filter_map(|(i, (route, _))| {
                    if (has_head && route.method == "HEAD")
                        || (!has_head && route.method == "GET")
                    {
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
                .filter_map(|(i, (route, _))| (route.method == method).then_some(i))
                .collect()
        };

        if eligible.is_empty() {
            let mut allowed: Vec<String> =
                candidates.iter().map(|(route, _)| route.method.clone()).collect();
            if allowed.iter().any(|m| m == "GET") {
                allowed.push("HEAD".to_owned());
            }
            allowed.sort();
            allowed.dedup();
            return Err(Error::MethodNotAllowed { allowed });
        }

        let mut best = eligible[0];
        for &i in eligible.iter().skip(1) {
            let (candidate, _) = &candidates[i];
            let (current, _) = &candidates[best];
            if candidate.rank > current.rank
                || (candidate.rank == current.rank && candidate.id < current.id)
            {
                best = i;
            }
        }

        let (route, params) = &candidates[best];
        let redirect = if self.policy == SlashPolicy::Redirect
            && !head.is_empty()
            && head != "/"
            && route.trailing_slash != request_trailing
        {
            let mut target = head.to_owned();
            if route.trailing_slash {
                target.push('/');
            } else {
                target.pop();
            }
            Some(target)
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

fn validate_method(method: &str) -> Result<(), Error> {
    if method.is_empty() || !method.bytes().all(|b| b.is_ascii_uppercase()) {
        Err(Error::InvalidMethod)
    } else {
        Ok(())
    }
}

fn valid_name(name: &str) -> bool {
    let mut bytes = name.bytes();
    match bytes.next() {
        Some(b'a'..=b'z') => {}
        _ => return false,
    }
    bytes.all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'_')
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
    let mut names = Vec::<String>::new();

    for (index, (part, start)) in parts.iter().enumerate() {
        if part.is_empty() {
            return Err(Error::InvalidPattern { at: *start });
        }
        if part.starts_with(':') || part.starts_with('*') {
            let marker = part.as_bytes()[0];
            let name = &part[1..];
            if !valid_name(name) || names.iter().any(|existing| existing == name) {
                return Err(Error::InvalidPattern { at: *start });
            }
            if marker == b'*' && index + 1 != parts.len() {
                return Err(Error::InvalidPattern { at: *start });
            }
            names.push(name.to_owned());
            if marker == b':' {
                segments.push(Segment::Param(name.to_owned()));
            } else {
                segments.push(Segment::Wildcard(name.to_owned()));
            }
        } else {
            let decoded = decode_static(part, *start).map_err(|at| Error::InvalidPattern { at })?;
            segments.push(Segment::Static(decoded));
        }
    }

    Ok((segments, trailing))
}

fn parse_path(path: &str) -> Result<(Vec<String>, bool), Error> {
    if !path.starts_with('/') {
        return Err(Error::InvalidPath { at: 0 });
    }
    if path == "/" {
        return Ok((Vec::new(), false));
    }

    let (parts, trailing) = split_nonroot(path);
    let mut segments = Vec::with_capacity(parts.len());
    for (part, start) in parts {
        if part.is_empty() {
            return Err(Error::InvalidPath { at: start });
        }
        segments.push(decode_static(part, start).map_err(|at| Error::InvalidPath { at })?);
    }
    Ok((segments, trailing))
}

fn split_nonroot(input: &str) -> (Vec<(&str, usize)>, bool) {
    let trailing = input.ends_with('/');
    let end = if trailing { input.len() - 1 } else { input.len() };
    let mut parts = Vec::new();
    let mut start = 1;
    let bytes = input.as_bytes();

    for i in 1..end {
        if bytes[i] == b'/' {
            parts.push((&input[start..i], start));
            start = i + 1;
        }
    }
    parts.push((&input[start..end], start));
    (parts, trailing)
}

fn decode_static(segment: &str, base: usize) -> Result<String, usize> {
    let bytes = segment.as_bytes();
    let mut decoded = Vec::with_capacity(bytes.len());
    let mut i = 0;

    while i < bytes.len() {
        let b = bytes[i];
        if b == b'%' {
            if i + 2 >= bytes.len() {
                return Err(base + i);
            }
            let hi = upper_hex(bytes[i + 1]).ok_or(base + i)?;
            let lo = upper_hex(bytes[i + 2]).ok_or(base + i)?;
            let value = (hi << 4) | lo;
            if !(0x20..=0x7e).contains(&value)
                || value == b'/'
                || is_unreserved(value)
            {
                return Err(base + i);
            }
            decoded.push(value);
            i += 3;
        } else {
            if !is_unreserved(b) {
                return Err(base + i);
            }
            decoded.push(b);
            i += 1;
        }
    }

    Ok(String::from_utf8(decoded).expect("decoded bytes are ASCII"))
}

fn upper_hex(b: u8) -> Option<u8> {
    match b {
        b'0'..=b'9' => Some(b - b'0'),
        b'A'..=b'F' => Some(b - b'A' + 10),
        _ => None,
    }
}

fn is_unreserved(b: u8) -> bool {
    b.is_ascii_alphanumeric() || matches!(b, b'.' | b'_' | b'~' | b'-')
}

fn match_route(pattern: &[Segment], path: &[String]) -> Option<Vec<(String, String)>> {
    let mut params = Vec::new();
    let mut pi = 0;

    for segment in pattern {
        match segment {
            Segment::Static(value) => {
                if path.get(pi)? != value {
                    return None;
                }
                pi += 1;
            }
            Segment::Param(name) => {
                let value = path.get(pi)?;
                params.push((name.clone(), value.clone()));
                pi += 1;
            }
            Segment::Wildcard(name) => {
                params.push((name.clone(), path[pi..].join("/")));
                return Some(params);
            }
        }
    }

    if pi == path.len() {
        Some(params)
    } else {
        None
    }
}

fn main() {}