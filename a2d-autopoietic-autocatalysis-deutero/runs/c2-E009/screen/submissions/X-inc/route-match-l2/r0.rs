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
    segments: Vec<Segment>,
    id: usize,
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

        for segment in &segments {
            match segment {
                Segment::Static(_) => rank.push(2),
                Segment::Param(_) => rank.push(1),
                Segment::Wildcard(_) => rank.push(0),
            }
        }

        if !matches!(segments.last(), Some(Segment::Wildcard(_))) {
            rank.push(3);
        }

        let id = self.routes.len();
        self.routes.push(Route { segments, id, rank });
        Ok(id)
    }

    pub fn resolve(&self, path: &str) -> Result<Match, Error> {
        let head_end = path
            .char_indices()
            .find(|(_, ch)| *ch == '?' || *ch == '#')
            .map(|(i, _)| i)
            .unwrap_or(path.len());
        let head = &path[..head_end];
        let path_segments = parse_path(head)?;

        let mut best: Option<(&Route, Vec<(String, String)>)> = None;

        for route in &self.routes {
            if let Some(params) = match_route(&route.segments, &path_segments) {
                let replace = match &best {
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

        match best {
            Some((route, params)) => Ok(Match {
                route: route.id,
                params,
            }),
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

            names.push(name.to_owned());
            if first == b':' {
                segments.push(Segment::Param(name.to_owned()));
            } else {
                segments.push(Segment::Wildcard(name.to_owned()));
            }
        } else {
            let decoded = decode_segment(raw, start).map_err(|at| Error::InvalidPattern { at })?;
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

        let raw = &path[start..end];
        let decoded = decode_segment(raw, start).map_err(|at| Error::InvalidPath { at })?;
        segments.push(decoded);

        if end == bytes.len() {
            break;
        }
        start = end + 1;
        if start == bytes.len() {
            return Err(Error::InvalidPath { at: end });
        }
    }

    Ok(segments)
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

fn decode_segment(raw: &str, base: usize) -> Result<String, usize> {
    let bytes = raw.as_bytes();
    let mut decoded = Vec::with_capacity(bytes.len());
    let mut i = 0;

    while i < bytes.len() {
        let byte = bytes[i];
        if byte == b'%' {
            if i + 2 >= bytes.len() {
                return Err(base + i);
            }
            let Some(high) = upper_hex(bytes[i + 1]) else {
                return Err(base + i);
            };
            let Some(low) = upper_hex(bytes[i + 2]) else {
                return Err(base + i);
            };
            let value = (high << 4) | low;
            if !(0x20..=0x7e).contains(&value)
                || value == b'/'
                || is_unreserved(value)
            {
                return Err(base + i);
            }
            decoded.push(value);
            i += 3;
        } else if is_unreserved(byte) {
            decoded.push(byte);
            i += 1;
        } else {
            return Err(base + i);
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

fn match_route(pattern: &[Segment], path: &[String]) -> Option<Vec<(String, String)>> {
    let mut params = Vec::new();
    let mut path_index = 0;

    for segment in pattern {
        match segment {
            Segment::Static(value) => {
                if path.get(path_index)? != value {
                    return None;
                }
                path_index += 1;
            }
            Segment::Param(name) => {
                params.push((name.clone(), path.get(path_index)?.clone()));
                path_index += 1;
            }
            Segment::Wildcard(name) => {
                let value = path[path_index..].join("/");
                params.push((name.clone(), value));
                return Some(params);
            }
        }
    }

    if path_index == path.len() {
        Some(params)
    } else {
        None
    }
}