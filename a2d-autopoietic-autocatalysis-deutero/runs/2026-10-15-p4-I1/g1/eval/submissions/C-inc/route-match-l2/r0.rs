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
enum Segment {
    Static(String),
    Param(String),
    Wildcard(String),
}

#[derive(Clone)]
struct Route {
    id: usize,
    segments: Vec<Segment>,
    rank: Vec<u8>,
}

pub struct Router {
    routes: Vec<Route>,
}

impl Router {
    pub fn new() -> Self {
        Self { routes: Vec::new() }
    }

    pub fn add(&mut self, pattern: &str) -> Result<usize, Error> {
        let segments = parse_pattern(pattern)?;
        let mut rank = Vec::with_capacity(segments.len() + 1);
        let has_wildcard = segments.iter().any(|s| matches!(s, Segment::Wildcard(_)));

        for segment in &segments {
            rank.push(match segment {
                Segment::Static(_) => 2,
                Segment::Param(_) => 1,
                Segment::Wildcard(_) => 0,
            });
        }
        if !has_wildcard {
            rank.push(3);
        }

        let id = self.routes.len();
        self.routes.push(Route { id, segments, rank });
        Ok(id)
    }

    pub fn resolve(&self, path: &str) -> Result<Match, Error> {
        let head_end = path
            .char_indices()
            .find_map(|(i, c)| matches!(c, '?' | '#').then_some(i))
            .unwrap_or(path.len());
        let head = &path[..head_end];
        let parts = parse_path(head)?;

        let mut best: Option<(&Route, Vec<(String, String)>)> = None;

        for route in &self.routes {
            if let Some(params) = match_route(route, &parts) {
                let replace = match best.as_ref() {
                    None => true,
                    Some((current, _)) => {
                        route.rank > current.rank
                            || (route.rank == current.rank && route.id < current.id)
                    }
                };
                if replace {
                    best = Some((route, params));
                }
            }
        }

        best.map(|(route, params)| Match {
            route: route.id,
            params,
        })
        .ok_or(Error::NotFound)
    }
}

fn parse_pattern(pattern: &str) -> Result<Vec<Segment>, Error> {
    if !pattern.starts_with('/') {
        return Err(Error::InvalidPattern { at: 0 });
    }
    if pattern == "/" {
        return Ok(Vec::new());
    }

    let bytes = pattern.as_bytes();
    let mut raw_segments = Vec::new();
    let mut start = 1;

    for i in 1..bytes.len() {
        if bytes[i] == b'/' {
            if i == start {
                return Err(Error::InvalidPattern { at: i });
            }
            raw_segments.push((start, i));
            start = i + 1;
        }
    }

    let trailing_slash = bytes.last() == Some(&b'/');
    if trailing_slash {
        if start == bytes.len() {
            return Err(Error::InvalidPattern { at: bytes.len() - 1 });
        }
        raw_segments.push((start, bytes.len() - 1));
        return Err(Error::InvalidPattern { at: bytes.len() - 1 });
    }
    if start == bytes.len() {
        return Err(Error::InvalidPattern { at: start });
    }
    raw_segments.push((start, bytes.len()));

    let mut result = Vec::with_capacity(raw_segments.len());
    let mut names = Vec::<String>::new();

    for (index, &(begin, end)) in raw_segments.iter().enumerate() {
        let segment = &pattern[begin..end];
        if segment.starts_with(':') || segment.starts_with('*') {
            let marker = segment.as_bytes()[0];
            let name = &segment[1..];
            if !valid_name(name) || names.iter().any(|n| n == name) {
                return Err(Error::InvalidPattern { at: begin });
            }
            if marker == b'*' && index + 1 != raw_segments.len() {
                return Err(Error::InvalidPattern { at: begin });
            }
            names.push(name.to_string());
            result.push(if marker == b':' {
                Segment::Param(name.to_string())
            } else {
                Segment::Wildcard(name.to_string())
            });
        } else {
            result.push(Segment::Static(decode_segment(segment, begin, true)?));
        }
    }

    Ok(result)
}

fn parse_path(path: &str) -> Result<Vec<String>, Error> {
    if !path.starts_with('/') {
        return Err(Error::InvalidPath { at: 0 });
    }
    if path == "/" {
        return Ok(Vec::new());
    }

    let bytes = path.as_bytes();
    let mut raw_segments = Vec::new();
    let mut start = 1;

    for i in 1..bytes.len() {
        if bytes[i] == b'/' {
            if i == start {
                return Err(Error::InvalidPath { at: i });
            }
            raw_segments.push((start, i));
            start = i + 1;
        }
    }

    if bytes.last() == Some(&b'/') {
        let slash_at = bytes.len() - 1;
        if start == bytes.len() {
            return Err(Error::InvalidPath { at: slash_at });
        }
        raw_segments.push((start, slash_at));
        // Validate preceding segments before reporting trailing slash.
        let mut decoded = Vec::with_capacity(raw_segments.len());
        for &(begin, end) in &raw_segments {
            decoded.push(decode_segment(&path[begin..end], begin, false)?);
        }
        return Err(Error::InvalidPath { at: slash_at });
    }

    if start == bytes.len() {
        return Err(Error::InvalidPath { at: start });
    }
    raw_segments.push((start, bytes.len()));

    raw_segments
        .into_iter()
        .map(|(begin, end)| decode_segment(&path[begin..end], begin, false))
        .collect()
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

fn decode_segment(segment: &str, offset: usize, _pattern: bool) -> Result<String, Error> {
    let bytes = segment.as_bytes();
    let mut decoded = Vec::with_capacity(bytes.len());
    let mut i = 0;

    while i < bytes.len() {
        let b = bytes[i];
        if b == b'%' {
            if i + 2 >= bytes.len()
                || !is_upper_hex(bytes[i + 1])
                || !is_upper_hex(bytes[i + 2])
            {
                return Err(Error::InvalidPath { at: offset + i });
            }
            let value = (hex_value(bytes[i + 1]) << 4) | hex_value(bytes[i + 2]);
            if !(0x20..=0x7e).contains(&value)
                || value == b'/'
                || is_unreserved(value)
            {
                return Err(Error::InvalidPath { at: offset + i });
            }
            decoded.push(value);
            i += 3;
        } else if is_unreserved(b) {
            decoded.push(b);
            i += 1;
        } else {
            return Err(Error::InvalidPath { at: offset + i });
        }
    }

    Ok(String::from_utf8(decoded).expect("decoded bytes are ASCII"))
}

fn is_unreserved(b: u8) -> bool {
    b.is_ascii_alphanumeric() || matches!(b, b'.' | b'_' | b'~' | b'-')
}

fn is_upper_hex(b: u8) -> bool {
    b.is_ascii_digit() || (b'A'..=b'F').contains(&b)
}

fn hex_value(b: u8) -> u8 {
    match b {
        b'0'..=b'9' => b - b'0',
        b'A'..=b'F' => b - b'A' + 10,
        _ => 0,
    }
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

fn main() {}