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

#[derive(Debug, Clone)]
enum Segment {
    Static(String),
    Param(String),
    Wildcard(String),
}

#[derive(Debug, Clone)]
struct Route {
    segments: Vec<Segment>,
    rank: Vec<u8>,
    id: usize,
}

pub struct Router {
    routes: Vec<Route>,
}

impl Router {
    pub fn new() -> Self {
        Self { routes: Vec::new() }
    }

    pub fn add(&mut self, pattern: &str) -> Result<usize, Error> {
        let parsed = parse_pattern(pattern)?;
        let id = self.routes.len();
        let mut rank = Vec::with_capacity(parsed.len() + 1);
        let has_wildcard = parsed
            .iter()
            .any(|segment| matches!(segment, Segment::Wildcard(_)));
        for segment in &parsed {
            rank.push(match segment {
                Segment::Static(_) => 2,
                Segment::Param(_) => 1,
                Segment::Wildcard(_) => 0,
            });
        }
        if !has_wildcard {
            rank.push(3);
        }
        self.routes.push(Route {
            segments: parsed,
            rank,
            id,
        });
        Ok(id)
    }

    pub fn resolve(&self, path: &str) -> Result<Match, Error> {
        let head_end = path
            .char_indices()
            .find_map(|(i, c)| if c == '?' || c == '#' { Some(i) } else { None })
            .unwrap_or(path.len());
        let head = &path[..head_end];
        let parts = parse_path(head)?;
        let mut best: Option<(&Route, Vec<(String, String)>)> = None;

        for route in &self.routes {
            if let Some(params) = match_route(route, &parts) {
                let replace = match &best {
                    None => true,
                    Some((best_route, _)) => {
                        route.rank > best_route.rank
                            || (route.rank == best_route.rank && route.id < best_route.id)
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

    let mut segments = Vec::new();
    let mut names = std::collections::HashSet::new();
    let bytes = pattern.as_bytes();
    let mut start = 1;

    while start < bytes.len() {
        let end = bytes[start..]
            .iter()
            .position(|&b| b == b'/')
            .map(|n| start + n)
            .unwrap_or(bytes.len());

        if end == start {
            if end == bytes.len() - 1 {
                return Err(Error::InvalidPattern { at: end });
            }
            return Err(Error::InvalidPattern { at: start });
        }

        let raw = &pattern[start..end];
        let first = bytes[start];
        if first == b':' || first == b'*' {
            let name = &raw[1..];
            if !valid_name(name) || !names.insert(name.to_string()) {
                return Err(Error::InvalidPattern { at: start });
            }
            if first == b'*' && end != bytes.len() {
                return Err(Error::InvalidPattern { at: start });
            }
            segments.push(if first == b':' {
                Segment::Param(name.to_string())
            } else {
                Segment::Wildcard(name.to_string())
            });
        } else {
            segments.push(Segment::Static(decode_segment(raw, start, true)?));
        }

        if end == bytes.len() {
            break;
        }
        if end == bytes.len() - 1 {
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
            .map(|n| start + n)
            .unwrap_or(bytes.len());

        if end == start {
            if end == bytes.len() - 1 {
                return Err(Error::InvalidPath { at: end });
            }
            return Err(Error::InvalidPath { at: start });
        }

        let raw = &path[start..end];
        segments.push(decode_segment(raw, start, false)?);

        if end == bytes.len() {
            break;
        }
        if end == bytes.len() - 1 {
            return Err(Error::InvalidPath { at: end });
        }
        start = end + 1;
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

fn decode_segment(raw: &str, base: usize, pattern: bool) -> Result<String, Error> {
    let bytes = raw.as_bytes();
    let mut decoded = Vec::with_capacity(bytes.len());
    let mut i = 0;

    while i < bytes.len() {
        let byte = bytes[i];
        if byte == b'%' {
            if i + 2 >= bytes.len() {
                return invalid_segment(pattern, base + i);
            }
            let Some(high) = uppercase_hex(bytes[i + 1]) else {
                return invalid_segment(pattern, base + i);
            };
            let Some(low) = uppercase_hex(bytes[i + 2]) else {
                return invalid_segment(pattern, base + i);
            };
            let value = (high << 4) | low;
            if !(0x20..=0x7e).contains(&value)
                || value == b'/'
                || is_unreserved(value)
            {
                return invalid_segment(pattern, base + i);
            }
            decoded.push(value);
            i += 3;
        } else if is_unreserved(byte) {
            decoded.push(byte);
            i += 1;
        } else {
            return invalid_segment(pattern, base + i);
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

fn invalid_segment<T>(pattern: bool, at: usize) -> Result<T, Error> {
    if pattern {
        Err(Error::InvalidPattern { at })
    } else {
        Err(Error::InvalidPath { at })
    }
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