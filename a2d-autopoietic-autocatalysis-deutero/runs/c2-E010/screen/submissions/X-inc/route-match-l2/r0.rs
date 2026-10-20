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
            .find_map(|(i, ch)| (ch == '?' || ch == '#').then_some(i))
            .unwrap_or(path.len());
        let head = &path[..head_end];
        let parts = parse_path(head)?;

        let mut best: Option<(&Route, Vec<(String, String)>)> = None;
        for route in &self.routes {
            let Some(params) = match_route(route, &parts) else {
                continue;
            };
            let replace = best
                .as_ref()
                .map(|(current, _)| route.rank > current.rank)
                .unwrap_or(true);
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

        let raw = &bytes[start..end];
        if raw[0] == b':' || raw[0] == b'*' {
            let is_wildcard = raw[0] == b'*';
            let name_bytes = &raw[1..];
            let valid_name = !name_bytes.is_empty()
                && name_bytes[0].is_ascii_lowercase()
                && name_bytes[1..]
                    .iter()
                    .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || *b == b'_');

            if !valid_name {
                return Err(Error::InvalidPattern { at: start });
            }

            let name = String::from_utf8(name_bytes.to_vec()).unwrap();
            if names.iter().any(|existing| existing == &name) {
                return Err(Error::InvalidPattern { at: start });
            }
            if is_wildcard && end < bytes.len() - 1 {
                return Err(Error::InvalidPattern { at: start });
            }
            names.push(name.clone());
            segments.push(if is_wildcard {
                Segment::Wildcard(name)
            } else {
                Segment::Param(name)
            });
        } else {
            segments.push(Segment::Static(decode_segment(raw, start, true)?));
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
        segments.push(decode_segment(&bytes[start..end], start, false)?);

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

fn decode_segment(raw: &[u8], base: usize, pattern: bool) -> Result<String, Error> {
    let invalid = |at| {
        if pattern {
            Error::InvalidPattern { at }
        } else {
            Error::InvalidPath { at }
        }
    };

    let mut decoded = Vec::with_capacity(raw.len());
    let mut i = 0;
    while i < raw.len() {
        let byte = raw[i];
        if byte == b'%' {
            if i + 2 >= raw.len()
                || !raw[i + 1].is_ascii_hexdigit()
                || !raw[i + 2].is_ascii_hexdigit()
                || raw[i + 1].is_ascii_lowercase()
                || raw[i + 2].is_ascii_lowercase()
            {
                return Err(invalid(base + i));
            }
            let high = hex_value(raw[i + 1]);
            let low = hex_value(raw[i + 2]);
            let value = high * 16 + low;
            if !(0x20..=0x7e).contains(&value)
                || value == b'/'
                || is_unreserved(value)
            {
                return Err(invalid(base + i));
            }
            decoded.push(value);
            i += 3;
        } else if is_unreserved(byte) {
            decoded.push(byte);
            i += 1;
        } else {
            return Err(invalid(base + i));
        }
    }

    Ok(String::from_utf8(decoded).unwrap())
}

fn hex_value(byte: u8) -> u8 {
    match byte {
        b'0'..=b'9' => byte - b'0',
        b'A'..=b'F' => byte - b'A' + 10,
        _ => 0,
    }
}

fn is_unreserved(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'_' | b'~' | b'-')
}

fn match_route(route: &Route, parts: &[String]) -> Option<Vec<(String, String)>> {
    let mut params = Vec::new();
    let mut path_index = 0;

    for segment in &route.segments {
        match segment {
            Segment::Static(value) => {
                if parts.get(path_index)? != value {
                    return None;
                }
                path_index += 1;
            }
            Segment::Param(name) => {
                params.push((name.clone(), parts.get(path_index)?.clone()));
                path_index += 1;
            }
            Segment::Wildcard(name) => {
                params.push((name.clone(), parts[path_index..].join("/")));
                path_index = parts.len();
                break;
            }
        }
    }

    (path_index == parts.len()).then_some(params)
}