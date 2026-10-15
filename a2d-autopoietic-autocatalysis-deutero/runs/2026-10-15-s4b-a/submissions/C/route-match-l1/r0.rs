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
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct Route {
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
        let rank = segments
            .iter()
            .map(|segment| match segment {
                Segment::Static(_) => 2,
                Segment::Param(_) => 1,
            })
            .collect();
        let id = self.routes.len();
        self.routes.push(Route { segments, rank });
        Ok(id)
    }

    pub fn resolve(&self, path: &str) -> Result<Match, Error> {
        let head_end = path
            .char_indices()
            .find(|(_, ch)| *ch == '?' || *ch == '#')
            .map(|(i, _)| i)
            .unwrap_or(path.len());
        let head = &path[..head_end];
        let values = parse_path(head)?;

        let mut best: Option<(usize, Vec<u8>, Vec<(String, String)>)> = None;
        for (route_id, route) in self.routes.iter().enumerate() {
            if route.segments.len() != values.len() {
                continue;
            }

            let mut params = Vec::new();
            let mut matches = true;
            for (pattern_segment, value) in route.segments.iter().zip(&values) {
                match pattern_segment {
                    Segment::Static(expected) if expected == value => {}
                    Segment::Static(_) => {
                        matches = false;
                        break;
                    }
                    Segment::Param(name) => params.push((name.clone(), value.clone())),
                }
            }

            if matches {
                let replace = best
                    .as_ref()
                    .map(|(_, best_rank, _)| route.rank > *best_rank)
                    .unwrap_or(true);
                if replace {
                    best = Some((route_id, route.rank.clone(), params));
                }
            }
        }

        match best {
            Some((route, _, params)) => Ok(Match { route, params }),
            None => Err(Error::NotFound),
        }
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
    let mut segments = Vec::new();
    let mut names = std::collections::HashSet::new();
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

        if first == b':' {
            let name = &raw[1..];
            if !valid_capture_name(name) || !names.insert(name.to_owned()) {
                return Err(Error::InvalidPattern { at: start });
            }
            segments.push(Segment::Param(name.to_owned()));
        } else if first == b'*' {
            return Err(Error::InvalidPattern { at: start });
        } else {
            let decoded = decode_segment(bytes, start, end)
                .map_err(|at| Error::InvalidPattern { at })?;
            segments.push(Segment::Static(decoded));
        }

        if end == bytes.len() {
            break;
        }
        if end + 1 == bytes.len() {
            return Err(Error::InvalidPattern { at: end });
        }
        start = end + 1;
    }

    Ok(segments)
}

fn parse_path(path: &str) -> Result<Vec<String>, Error> {
    if !path.starts_with('/') {
        return Err(Error::InvalidPath { at: 0 });
    }
    if path == "/" {
        return Ok(Vec::new());
    }

    let bytes = path.as_bytes();
    let mut segments = Vec::new();
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

        let decoded =
            decode_segment(bytes, start, end).map_err(|at| Error::InvalidPath { at })?;
        segments.push(decoded);

        if end == bytes.len() {
            break;
        }
        if end + 1 == bytes.len() {
            return Err(Error::InvalidPath { at: end });
        }
        start = end + 1;
    }

    Ok(segments)
}

fn valid_capture_name(name: &str) -> bool {
    let bytes = name.as_bytes();
    if bytes.is_empty() || !bytes[0].is_ascii_lowercase() {
        return false;
    }
    bytes[1..]
        .iter()
        .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || *b == b'_')
}

fn decode_segment(bytes: &[u8], start: usize, end: usize) -> Result<String, usize> {
    let mut decoded = Vec::with_capacity(end - start);
    let mut i = start;

    while i < end {
        let byte = bytes[i];
        if byte == b'%' {
            if i + 2 >= end {
                return Err(i);
            }
            let hi = upper_hex(bytes[i + 1]).ok_or(i)?;
            let lo = upper_hex(bytes[i + 2]).ok_or(i)?;
            let value = (hi << 4) | lo;
            if !(0x20..=0x7e).contains(&value)
                || value == b'/'
                || is_unreserved(value)
            {
                return Err(i);
            }
            decoded.push(value);
            i += 3;
        } else if is_unreserved(byte) {
            decoded.push(byte);
            i += 1;
        } else {
            return Err(i);
        }
    }

    Ok(String::from_utf8(decoded).expect("decoded bytes are ASCII"))
}

fn upper_hex(byte: u8) -> Option<u8> {
    match byte {
        b'0'..=b'9' => Some(byte - b'0'),
        b'A'..=b'F' => Some(byte - b'A' + 10),
        _ => None,
    }
}

fn is_unreserved(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'_' | b'~' | b'-')
}

fn main() {}