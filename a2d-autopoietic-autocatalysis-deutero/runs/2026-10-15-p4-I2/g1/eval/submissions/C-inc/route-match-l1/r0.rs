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
    id: usize,
    segments: Vec<Segment>,
    rank: Vec<u8>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Router {
    routes: Vec<Route>,
}

impl Router {
    pub fn new() -> Self {
        Self { routes: Vec::new() }
    }

    pub fn add(&mut self, pattern: &str) -> Result<usize, Error> {
        let segments = parse_pattern(pattern)?;
        let id = self.routes.len();
        let rank = segments
            .iter()
            .map(|segment| match segment {
                Segment::Static(_) => 2,
                Segment::Param(_) => 1,
            })
            .collect();

        self.routes.push(Route { id, segments, rank });
        Ok(id)
    }

    pub fn resolve(&self, path: &str) -> Result<Match, Error> {
        let head_end = path
            .char_indices()
            .find_map(|(i, c)| (c == '?' || c == '#').then_some(i))
            .unwrap_or(path.len());
        let head = &path[..head_end];
        let segments = parse_path(head)?;

        let mut best: Option<(&Route, Vec<(String, String)>)> = None;

        for route in &self.routes {
            if route.segments.len() != segments.len() {
                continue;
            }

            let mut params = Vec::new();
            let mut matched = true;

            for (pattern_segment, value) in route.segments.iter().zip(&segments) {
                match pattern_segment {
                    Segment::Static(expected) if expected == value => {}
                    Segment::Static(_) => {
                        matched = false;
                        break;
                    }
                    Segment::Param(name) => params.push((name.clone(), value.clone())),
                }
            }

            if !matched {
                continue;
            }

            let replace = match best {
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

        match best {
            Some((route, params)) => Ok(Match {
                route: route.id,
                params,
            }),
            None => Err(Error::NotFound),
        }
    }
}

impl Default for Router {
    fn default() -> Self {
        Self::new()
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

    for end in 1..=bytes.len() {
        if end == bytes.len() || bytes[end] == b'/' {
            if end == start {
                return Err(Error::InvalidPattern { at: start });
            }

            let raw = &pattern[start..end];
            if raw.as_bytes()[0] == b':' {
                let name = &raw[1..];
                if !valid_capture_name(name) || !names.insert(name.to_string()) {
                    return Err(Error::InvalidPattern { at: start });
                }
                segments.push(Segment::Param(name.to_string()));
            } else if raw.as_bytes()[0] == b'*' {
                return Err(Error::InvalidPattern { at: start });
            } else {
                let decoded = decode_static(raw, start).map_err(|at| Error::InvalidPattern { at })?;
                segments.push(Segment::Static(decoded));
            }

            if end == bytes.len() {
                break;
            }
            start = end + 1;
            if start == bytes.len() {
                return Err(Error::InvalidPattern { at: end });
            }
        }
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

    for end in 1..=bytes.len() {
        if end == bytes.len() || bytes[end] == b'/' {
            if end == start {
                return Err(Error::InvalidPath { at: start });
            }

            let raw = &path[start..end];
            let decoded = decode_static(raw, start).map_err(|at| Error::InvalidPath { at })?;
            segments.push(decoded);

            if end == bytes.len() {
                break;
            }
            start = end + 1;
            if start == bytes.len() {
                return Err(Error::InvalidPath { at: end });
            }
        }
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

fn decode_static(raw: &str, offset: usize) -> Result<String, usize> {
    let bytes = raw.as_bytes();
    let mut decoded = Vec::with_capacity(bytes.len());
    let mut i = 0;

    while i < bytes.len() {
        let byte = bytes[i];

        if byte == b'%' {
            if i + 2 >= bytes.len() {
                return Err(offset + i);
            }
            let hi = hex_upper(bytes[i + 1]).ok_or(offset + i)?;
            let lo = hex_upper(bytes[i + 2]).ok_or(offset + i)?;
            let value = (hi << 4) | lo;

            if !(0x20..=0x7e).contains(&value)
                || value == b'/'
                || is_unreserved(value)
            {
                return Err(offset + i);
            }

            decoded.push(value);
            i += 3;
            continue;
        }

        if !is_unreserved(byte) {
            return Err(offset + i);
        }
        decoded.push(byte);
        i += 1;
    }

    // The accepted byte set is ASCII, so this conversion is infallible.
    Ok(String::from_utf8(decoded).expect("validated ASCII bytes"))
}

fn is_unreserved(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'_' | b'~' | b'-')
}

fn hex_upper(byte: u8) -> Option<u8> {
    match byte {
        b'0'..=b'9' => Some(byte - b'0'),
        b'A'..=b'F' => Some(byte - b'A' + 10),
        _ => None,
    }
}

fn main() {}