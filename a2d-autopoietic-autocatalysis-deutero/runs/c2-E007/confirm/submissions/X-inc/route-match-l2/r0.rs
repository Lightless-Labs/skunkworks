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
        let has_wildcard = segments
            .iter()
            .any(|segment| matches!(segment, Segment::Wildcard(_)));

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
                    Some((current, _)) => route.rank > current.rank,
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
    let mut result = Vec::new();
    let mut names = Vec::<String>::new();
    let mut start = 1;

    while start < bytes.len() {
        let end = bytes[start..]
            .iter()
            .position(|&b| b == b'/')
            .map(|offset| start + offset)
            .unwrap_or(bytes.len());

        if end == start {
            return Err(Error::InvalidPattern { at: start });
        }

        let raw = &pattern[start..end];
        let first = bytes[start];

        if first == b':' || first == b'*' {
            let name = &raw[1..];
            if !valid_name(name) || names.iter().any(|existing| existing == name) {
                return Err(Error::InvalidPattern { at: start });
            }
            if first == b'*' && end != bytes.len() {
                return Err(Error::InvalidPattern { at: start });
            }
            names.push(name.to_string());
            if first == b':' {
                result.push(Segment::Param(name.to_string()));
            } else {
                result.push(Segment::Wildcard(name.to_string()));
            }
        } else {
            result.push(Segment::Static(decode_segment(raw, start, true)?));
        }

        if end == bytes.len() {
            break;
        }

        if end + 1 == bytes.len() {
            return Err(Error::InvalidPattern { at: end });
        }
        start = end + 1;
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
    let mut result = Vec::new();
    let mut start = 1;

    while start < bytes.len() {
        let end = bytes[start..]
            .iter()
            .position(|&b| b == b'/')
            .map(|offset| start + offset)
            .unwrap_or(bytes.len());

        if end == start {
            return Err(Error::InvalidPath { at: start });
        }

        result.push(decode_segment(&path[start..end], start, false)?);

        if end == bytes.len() {
            break;
        }
        if end + 1 == bytes.len() {
            return Err(Error::InvalidPath { at: end });
        }
        start = end + 1;
    }

    Ok(result)
}

fn valid_name(name: &str) -> bool {
    let bytes = name.as_bytes();
    if bytes.is_empty() || !bytes[0].is_ascii_lowercase() {
        return false;
    }
    bytes[1..]
        .iter()
        .all(|&b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'_')
}

fn decode_segment(raw: &str, base: usize, pattern: bool) -> Result<String, Error> {
    let bytes = raw.as_bytes();
    let mut decoded = Vec::with_capacity(bytes.len());
    let mut i = 0;

    while i < bytes.len() {
        let b = bytes[i];
        if b == b'%' {
            let invalid = || {
                if pattern {
                    Error::InvalidPattern { at: base + i }
                } else {
                    Error::InvalidPath { at: base + i }
                }
            };
            if i + 2 >= bytes.len() {
                return Err(invalid());
            }
            let hi = uppercase_hex(bytes[i + 1]).ok_or_else(invalid)?;
            let lo = uppercase_hex(bytes[i + 2]).ok_or_else(invalid)?;
            let value = (hi << 4) | lo;
            if !(0x20..=0x7e).contains(&value)
                || value == b'/'
                || is_unreserved(value)
            {
                return Err(invalid());
            }
            decoded.push(value);
            i += 3;
        } else if is_unreserved(b) {
            decoded.push(b);
            i += 1;
        } else {
            return Err(if pattern {
                Error::InvalidPattern { at: base + i }
            } else {
                Error::InvalidPath { at: base + i }
            });
        }
    }

    Ok(String::from_utf8(decoded).expect("decoded bytes are ASCII"))
}

fn uppercase_hex(byte: u8) -> Option<u8> {
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
    let mut params = Vec::new();
    let mut path_index = 0;

    for segment in &route.segments {
        match segment {
            Segment::Static(expected) => {
                if path.get(path_index)? != expected {
                    return None;
                }
                path_index += 1;
            }
            Segment::Param(name) => {
                let value = path.get(path_index)?;
                params.push((name.clone(), value.clone()));
                path_index += 1;
            }
            Segment::Wildcard(name) => {
                let value = path[path_index..].join("/");
                params.push((name.clone(), value));
                path_index = path.len();
                break;
            }
        }
    }

    if path_index == path.len() {
        Some(params)
    } else {
        None
    }
}