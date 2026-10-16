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

#[derive(Debug, Clone, PartialEq, Eq)]
enum Segment {
    Static(String),
    Param(String),
    Wildcard(String),
}

#[derive(Debug, Clone)]
struct Route {
    segments: Vec<Segment>,
    rank: Vec<u8>,
}

#[derive(Debug, Clone)]
struct ParsedPath {
    segments: Vec<String>,
}

#[derive(Debug, Clone)]
pub struct Router {
    routes: Vec<Route>,
}

impl Router {
    pub fn new() -> Self {
        Self { routes: Vec::new() }
    }

    pub fn add(&mut self, pattern: &str) -> Result<usize, Error> {
        let (segments, rank) = parse_pattern(pattern)?;
        let id = self.routes.len();
        self.routes.push(Route { segments, rank });
        Ok(id)
    }

    pub fn resolve(&self, path: &str) -> Result<Match, Error> {
        let head_end = path
            .char_indices()
            .find_map(|(i, c)| if c == '?' || c == '#' { Some(i) } else { None })
            .unwrap_or(path.len());
        let head = &path[..head_end];
        let parsed = parse_path(head)?;

        let mut best: Option<(usize, Vec<u8>, Vec<(String, String)>)> = None;

        for (id, route) in self.routes.iter().enumerate() {
            if let Some(params) = match_route(route, &parsed.segments) {
                match &best {
                    None => best = Some((id, route.rank.clone(), params)),
                    Some((best_id, best_rank, _)) => {
                        if route.rank > *best_rank
                            || (route.rank == *best_rank && id < *best_id)
                        {
                            best = Some((id, route.rank.clone(), params));
                        }
                    }
                }
            }
        }

        match best {
            Some((route, _, params)) => Ok(Match { route, params }),
            None => Err(Error::NotFound),
        }
    }
}

fn parse_pattern(pattern: &str) -> Result<(Vec<Segment>, Vec<u8>), Error> {
    if !pattern.starts_with('/') {
        return Err(Error::InvalidPattern { at: 0 });
    }
    if pattern == "/" {
        return Ok((Vec::new(), vec![3]));
    }

    let bytes = pattern.as_bytes();
    let mut parts = Vec::new();
    let mut start = 1;
    let mut i = 1;

    while i < bytes.len() {
        if bytes[i] == b'/' {
            if i == start {
                return Err(Error::InvalidPattern { at: start });
            }
            parts.push((start, &pattern[start..i]));
            start = i + 1;
        }
        i += 1;
    }

    let trailing_slash = bytes.last() == Some(&b'/');
    if !trailing_slash {
        if start == bytes.len() {
            return Err(Error::InvalidPattern { at: start });
        }
        parts.push((start, &pattern[start..]));
    }

    let mut segments = Vec::new();
    let mut rank = Vec::new();
    let mut names = std::collections::HashSet::new();

    for (idx, (at, raw)) in parts.iter().enumerate() {
        if raw.starts_with(':') || raw.starts_with('*') {
            let marker = raw.as_bytes()[0];
            let name = &raw[1..];
            if !valid_name(name) || !names.insert(name.to_owned()) {
                return Err(Error::InvalidPattern { at: *at });
            }
            if marker == b'*' && idx + 1 != parts.len() {
                return Err(Error::InvalidPattern { at: *at });
            }
            if marker == b':' {
                segments.push(Segment::Param(name.to_owned()));
                rank.push(1);
            } else {
                segments.push(Segment::Wildcard(name.to_owned()));
                rank.push(0);
            }
        } else {
            let decoded = decode_segment(raw, *at).map_err(|at| Error::InvalidPattern { at })?;
            segments.push(Segment::Static(decoded));
            rank.push(2);
        }
    }

    if trailing_slash {
        return Err(Error::InvalidPattern {
            at: bytes.len() - 1,
        });
    }

    if !segments.iter().any(|s| matches!(s, Segment::Wildcard(_))) {
        rank.push(3);
    }

    Ok((segments, rank))
}

fn parse_path(path: &str) -> Result<ParsedPath, Error> {
    if !path.starts_with('/') {
        return Err(Error::InvalidPath { at: 0 });
    }
    if path == "/" {
        return Ok(ParsedPath { segments: Vec::new() });
    }

    let bytes = path.as_bytes();
    let mut segments = Vec::new();
    let mut start = 1;
    let mut i = 1;

    while i < bytes.len() {
        if bytes[i] == b'/' {
            if i == start {
                return Err(Error::InvalidPath { at: start });
            }
            let raw = &path[start..i];
            segments.push(decode_segment(raw, start).map_err(|at| Error::InvalidPath { at })?);
            start = i + 1;
        }
        i += 1;
    }

    if bytes.last() == Some(&b'/') {
        if start < bytes.len() - 1 {
            let raw = &path[start..bytes.len() - 1];
            segments.push(decode_segment(raw, start).map_err(|at| Error::InvalidPath { at })?);
        } else if start == bytes.len() - 1 && start != 1 {
            return Err(Error::InvalidPath { at: start });
        }
        return Err(Error::InvalidPath {
            at: bytes.len() - 1,
        });
    }

    if start == bytes.len() {
        return Err(Error::InvalidPath { at: start });
    }
    segments.push(decode_segment(&path[start..], start).map_err(|at| Error::InvalidPath { at })?);
    Ok(ParsedPath { segments })
}

fn valid_name(name: &str) -> bool {
    let bytes = name.as_bytes();
    if bytes.is_empty() || !bytes[0].is_ascii_lowercase() {
        return false;
    }
    bytes[1..]
        .iter()
        .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || *b == b'_')
}

fn is_unreserved(b: u8) -> bool {
    b.is_ascii_alphanumeric() || matches!(b, b'.' | b'_' | b'~' | b'-')
}

fn hex_value(b: u8) -> Option<u8> {
    match b {
        b'0'..=b'9' => Some(b - b'0'),
        b'A'..=b'F' => Some(b - b'A' + 10),
        _ => None,
    }
}

fn decode_segment(raw: &str, offset: usize) -> Result<String, usize> {
    let bytes = raw.as_bytes();
    let mut decoded = Vec::with_capacity(bytes.len());
    let mut i = 0;

    while i < bytes.len() {
        let b = bytes[i];
        if b == b'%' {
            if i + 2 >= bytes.len() {
                return Err(offset + i);
            }
            let hi = hex_value(bytes[i + 1]).ok_or(offset + i)?;
            let lo = hex_value(bytes[i + 2]).ok_or(offset + i)?;
            let value = (hi << 4) | lo;
            if !(0x20..=0x7e).contains(&value)
                || value == b'/'
                || is_unreserved(value)
            {
                return Err(offset + i);
            }
            decoded.push(value);
            i += 3;
        } else {
            if !is_unreserved(b) {
                return Err(offset + i);
            }
            decoded.push(b);
            i += 1;
        }
    }

    Ok(String::from_utf8(decoded).expect("decoded segments are ASCII"))
}

fn match_route(route: &Route, path: &[String]) -> Option<Vec<(String, String)>> {
    let mut params = Vec::new();
    let mut pi = 0;

    for segment in &route.segments {
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
                let value = path[pi..].join("/");
                params.push((name.clone(), value));
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