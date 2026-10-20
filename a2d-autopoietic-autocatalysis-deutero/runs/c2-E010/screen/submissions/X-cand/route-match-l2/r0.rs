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
    segments: Vec<Segment>,
    has_wildcard: bool,
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
        let segments = parse_pattern(pattern)?;
        let id = self.routes.len();
        let has_wildcard = segments
            .iter()
            .any(|segment| matches!(segment, Segment::Wildcard(_)));
        self.routes.push(Route {
            segments,
            has_wildcard,
            id,
        });
        Ok(id)
    }

    pub fn resolve(&self, path: &str) -> Result<Match, Error> {
        let head_end = path
            .char_indices()
            .find_map(|(i, ch)| if ch == '?' || ch == '#' { Some(i) } else { None })
            .unwrap_or(path.len());
        let head = &path[..head_end];
        let parts = parse_path(head)?;

        let mut best: Option<(Vec<u8>, usize, Vec<(String, String)>)> = None;

        for route in &self.routes {
            let Some(params) = match_route(route, &parts) else {
                continue;
            };
            let rank = route_rank(route);
            let replace = match &best {
                None => true,
                Some((best_rank, best_id, _)) => {
                    rank > *best_rank || (rank == *best_rank && route.id < *best_id)
                }
            };
            if replace {
                best = Some((rank, route.id, params));
            }
        }

        match best {
            Some((_, route, params)) => Ok(Match { route, params }),
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
    let mut names = Vec::<String>::new();
    let bytes = pattern.as_bytes();
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
            if first == b'*' && end < bytes.len() && end + 1 < bytes.len() {
                return Err(Error::InvalidPattern { at: start });
            }
            if !valid_name(name) || names.iter().any(|existing| existing == name) {
                return Err(Error::InvalidPattern { at: start });
            }
            names.push(name.to_string());
            if first == b':' {
                segments.push(Segment::Param(name.to_string()));
            } else {
                segments.push(Segment::Wildcard(name.to_string()));
            }
        } else {
            let decoded = decode_segment(raw, start).map_err(|at| Error::InvalidPattern { at })?;
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
    let mut parts = Vec::new();
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

        let decoded = decode_segment(&path[start..end], start)
            .map_err(|at| Error::InvalidPath { at })?;
        parts.push(decoded);

        if end == bytes.len() {
            break;
        }
        if end + 1 == bytes.len() {
            return Err(Error::InvalidPath { at: end });
        }
        start = end + 1;
    }

    Ok(parts)
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
        let b = bytes[i];
        if b == b'%' {
            if i + 2 >= bytes.len() {
                return Err(base + i);
            }
            let Some(hi) = upper_hex(bytes[i + 1]) else {
                return Err(base + i);
            };
            let Some(lo) = upper_hex(bytes[i + 2]) else {
                return Err(base + i);
            };
            let value = (hi << 4) | lo;
            if !(0x20..=0x7e).contains(&value)
                || value == b'/'
                || is_unreserved(value)
            {
                return Err(base + i);
            }
            decoded.push(value);
            i += 3;
        } else if is_unreserved(b) {
            decoded.push(b);
            i += 1;
        } else {
            return Err(base + i);
        }
    }

    Ok(String::from_utf8(decoded).expect("decoded segments are ASCII"))
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

fn route_rank(route: &Route) -> Vec<u8> {
    let mut rank = Vec::with_capacity(route.segments.len() + 1);
    for segment in &route.segments {
        rank.push(match segment {
            Segment::Static(_) => 2,
            Segment::Param(_) => 1,
            Segment::Wildcard(_) => 0,
        });
        if matches!(segment, Segment::Wildcard(_)) {
            return rank;
        }
    }
    if !route.has_wildcard {
        rank.push(3);
    }
    rank
}
