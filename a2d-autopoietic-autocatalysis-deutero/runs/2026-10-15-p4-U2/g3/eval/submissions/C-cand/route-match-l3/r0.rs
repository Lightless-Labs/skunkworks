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
    id: usize,
    method: String,
    segments: Vec<Segment>,
    trailing_slash: bool,
    rank: Vec<u8>,
}

#[derive(Debug, Clone)]
struct ParsedPath {
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
        let id = self.routes.len();
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
            .char_indices()
            .find_map(|(i, c)| (c == '?' || c == '#').then_some(i))
            .unwrap_or(path.len());
        let head = &path[..head_end];
        let parsed = parse_path(head)?;

        let mut candidates: Vec<(&Route, Vec<(String, String)>)> = Vec::new();
        for route in &self.routes {
            if self.policy == SlashPolicy::Strict
                && route.trailing_slash != parsed.trailing_slash
            {
                continue;
            }
            if let Some(params) = match_route(route, &parsed.segments) {
                candidates.push((route, params));
            }
        }

        if candidates.is_empty() {
            return Err(Error::NotFound);
        }

        let eligible_method = if method == "HEAD"
            && candidates.iter().any(|(route, _)| route.method == "HEAD")
        {
            "HEAD"
        } else if method == "HEAD" {
            "GET"
        } else {
            method
        };

        let eligible: Vec<_> = candidates
            .iter()
            .filter(|(route, _)| route.method == eligible_method)
            .collect();

        if eligible.is_empty() {
            let mut allowed: Vec<String> = candidates
                .iter()
                .map(|(route, _)| route.method.clone())
                .collect();
            if allowed.iter().any(|m| m == "GET") {
                allowed.push("HEAD".to_owned());
            }
            allowed.sort();
            allowed.dedup();
            return Err(Error::MethodNotAllowed { allowed });
        }

        let (winner, params) = eligible
            .into_iter()
            .max_by(|(a, _), (b, _)| {
                a.rank
                    .cmp(&b.rank)
                    .then_with(|| b.id.cmp(&a.id))
            })
            .unwrap();

        let redirect = if self.policy == SlashPolicy::Redirect
            && !parsed.segments.is_empty()
            && winner.trailing_slash != parsed.trailing_slash
        {
            let mut target = head.to_owned();
            if winner.trailing_slash {
                target.push('/');
            } else {
                target.pop();
            }
            Some(target)
        } else {
            None
        };

        Ok(Match {
            route: winner.id,
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

fn parse_pattern(pattern: &str) -> Result<(Vec<Segment>, bool), Error> {
    if !pattern.starts_with('/') {
        return Err(Error::InvalidPattern { at: 0 });
    }
    if pattern == "/" {
        return Ok((Vec::new(), false));
    }

    let bytes = pattern.as_bytes();
    let mut raw_segments = Vec::new();
    let mut start = 1;
    for i in 1..bytes.len() {
        if bytes[i] == b'/' {
            raw_segments.push((start, i));
            start = i + 1;
        }
    }

    let mut trailing_slash = false;
    if start == bytes.len() {
        trailing_slash = true;
    } else {
        raw_segments.push((start, bytes.len()));
    }

    let mut segments = Vec::with_capacity(raw_segments.len());
    let mut names = std::collections::HashSet::new();

    for (index, &(begin, end)) in raw_segments.iter().enumerate() {
        if begin == end {
            return Err(Error::InvalidPattern { at: begin });
        }
        let raw = &pattern[begin..end];
        let first = raw.as_bytes()[0];

        if first == b':' || first == b'*' {
            let name = &raw[1..];
            if !valid_capture_name(name) || !names.insert(name.to_owned()) {
                return Err(Error::InvalidPattern { at: begin });
            }
            if first == b'*' && index + 1 != raw_segments.len() {
                return Err(Error::InvalidPattern { at: begin });
            }
            segments.push(if first == b':' {
                Segment::Param(name.to_owned())
            } else {
                Segment::Wildcard(name.to_owned())
            });
        } else {
            let decoded = decode_static_segment(raw, begin)
                .map_err(|at| Error::InvalidPattern { at })?;
            segments.push(Segment::Static(decoded));
        }
    }

    Ok((segments, trailing_slash))
}

fn parse_path(path: &str) -> Result<ParsedPath, Error> {
    if !path.starts_with('/') {
        return Err(Error::InvalidPath { at: 0 });
    }
    if path == "/" {
        return Ok(ParsedPath {
            segments: Vec::new(),
            trailing_slash: false,
        });
    }

    let bytes = path.as_bytes();
    let mut bounds = Vec::new();
    let mut start = 1;
    for i in 1..bytes.len() {
        if bytes[i] == b'/' {
            bounds.push((start, i));
            start = i + 1;
        }
    }

    let mut trailing_slash = false;
    if start == bytes.len() {
        trailing_slash = true;
    } else {
        bounds.push((start, bytes.len()));
    }

    let mut segments = Vec::with_capacity(bounds.len());
    for (begin, end) in bounds {
        if begin == end {
            return Err(Error::InvalidPath { at: begin });
        }
        let decoded = decode_static_segment(&path[begin..end], begin)
            .map_err(|at| Error::InvalidPath { at })?;
        segments.push(decoded);
    }

    Ok(ParsedPath {
        segments,
        trailing_slash,
    })
}

fn valid_capture_name(name: &str) -> bool {
    let bytes = name.as_bytes();
    !bytes.is_empty()
        && bytes[0].is_ascii_lowercase()
        && bytes[1..]
            .iter()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || *b == b'_')
}

fn decode_static_segment(raw: &str, base: usize) -> Result<String, usize> {
    let bytes = raw.as_bytes();
    let mut decoded = Vec::with_capacity(bytes.len());
    let mut i = 0;

    while i < bytes.len() {
        let byte = bytes[i];
        if byte == b'%' {
            if i + 2 >= bytes.len() {
                return Err(base + i);
            }
            let hi = hex_upper(bytes[i + 1]).ok_or(base + i)?;
            let lo = hex_upper(bytes[i + 2]).ok_or(base + i)?;
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
            if !is_unreserved(byte) {
                return Err(base + i);
            }
            decoded.push(byte);
            i += 1;
        }
    }

    String::from_utf8(decoded).map_err(|_| base)
}

fn hex_upper(byte: u8) -> Option<u8> {
    match byte {
        b'0'..=b'9' => Some(byte - b'0'),
        b'A'..=b'F' => Some(byte - b'A' + 10),
        _ => None,
    }
}

fn is_unreserved(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'_' | b'~' | b'-')
}

fn match_route(route: &Route, path: &[String]) -> Option<Vec<(String, String)>> {
    let has_wildcard = route
        .segments
        .last()
        .is_some_and(|segment| matches!(segment, Segment::Wildcard(_)));

    if !has_wildcard && route.segments.len() != path.len() {
        return None;
    }
    if has_wildcard && path.len() + 1 < route.segments.len() {
        return None;
    }

    let mut params = Vec::new();
    let fixed_len = if has_wildcard {
        route.segments.len() - 1
    } else {
        route.segments.len()
    };

    for i in 0..fixed_len {
        match &route.segments[i] {
            Segment::Static(value) if value == &path[i] => {}
            Segment::Param(name) => params.push((name.clone(), path[i].clone())),
            _ => return None,
        }
    }

    if has_wildcard {
        let Segment::Wildcard(name) = &route.segments[route.segments.len() - 1] else {
            unreachable!()
        };
        let value = path[fixed_len..].join("/");
        params.push((name.clone(), value));
    }

    Some(params)
}

fn main() {}