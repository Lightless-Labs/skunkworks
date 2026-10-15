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
        let id = self.routes.len();

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

        self.routes.push(Route { id, segments, rank });
        Ok(id)
    }

    pub fn resolve(&self, path: &str) -> Result<Match, Error> {
        let head_end = path
            .char_indices()
            .find_map(|(i, c)| if c == '?' || c == '#' { Some(i) } else { None })
            .unwrap_or(path.len());
        let head = &path[..head_end];
        let parts = parse_path(head)?;

        let mut best: Option<(usize, Vec<(String, String)>)> = None;

        for route in &self.routes {
            if let Some(params) = match_route(&route.segments, &parts) {
                let replace = match &best {
                    None => true,
                    Some((best_index, _)) => {
                        let best_route = &self.routes[*best_index];
                        route.rank > best_route.rank
                            || (route.rank == best_route.rank && route.id < best_route.id)
                    }
                };
                if replace {
                    best = Some((route.id, params));
                }
            }
        }

        match best {
            Some((route, params)) => Ok(Match { route, params }),
            None => Err(Error::NotFound),
        }
    }
}

fn parse_pattern(input: &str) -> Result<Vec<Segment>, Error> {
    if !input.starts_with('/') {
        return Err(Error::InvalidPattern { at: 0 });
    }
    if input == "/" {
        return Ok(Vec::new());
    }

    let bytes = input.as_bytes();
    let mut result = Vec::new();
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

        let raw = &input[start..end];
        let is_last = end == bytes.len();

        if raw.as_bytes()[0] == b':' || raw.as_bytes()[0] == b'*' {
            let marker = raw.as_bytes()[0];
            let name = &raw[1..];

            if !valid_name(name) || result.iter().any(|s| segment_name(s) == Some(name)) {
                return Err(Error::InvalidPattern { at: start });
            }
            if marker == b'*' && !is_last {
                return Err(Error::InvalidPattern { at: start });
            }

            result.push(if marker == b':' {
                Segment::Param(name.to_string())
            } else {
                Segment::Wildcard(name.to_string())
            });
        } else {
            let decoded = decode_segment(raw, start).map_err(|at| Error::InvalidPattern { at })?;
            result.push(Segment::Static(decoded));
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

fn parse_path(input: &str) -> Result<Vec<String>, Error> {
    if !input.starts_with('/') {
        return Err(Error::InvalidPath { at: 0 });
    }
    if input == "/" {
        return Ok(Vec::new());
    }

    let bytes = input.as_bytes();
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

        let decoded = decode_segment(&input[start..end], start)
            .map_err(|at| Error::InvalidPath { at })?;
        result.push(decoded);

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
        .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || *b == b'_')
}

fn segment_name(segment: &Segment) -> Option<&str> {
    match segment {
        Segment::Param(name) | Segment::Wildcard(name) => Some(name),
        Segment::Static(_) => None,
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
            let hi = hex_upper(bytes[i + 1]).ok_or(offset + i)?;
            let lo = hex_upper(bytes[i + 2]).ok_or(offset + i)?;
            let value = hi * 16 + lo;
            if !(0x20..=0x7e).contains(&value)
                || value == b'/'
                || is_unreserved(value)
            {
                return Err(offset + i);
            }
            decoded.push(value);
            i += 3;
        } else if is_unreserved(b) {
            decoded.push(b);
            i += 1;
        } else {
            return Err(offset + i);
        }
    }

    Ok(String::from_utf8(decoded).expect("decoded bytes are ASCII"))
}

fn hex_upper(b: u8) -> Option<u8> {
    match b {
        b'0'..=b'9' => Some(b - b'0'),
        b'A'..=b'F' => Some(b - b'A' + 10),
        _ => None,
    }
}

fn is_unreserved(b: u8) -> bool {
    b.is_ascii_alphanumeric() || matches!(b, b'.' | b'_' | b'~' | b'-')
}

fn match_route(segments: &[Segment], parts: &[String]) -> Option<Vec<(String, String)>> {
    let mut params = Vec::new();
    let mut path_index = 0;

    for segment in segments {
        match segment {
            Segment::Static(expected) => {
                if parts.get(path_index)? != expected {
                    return None;
                }
                path_index += 1;
            }
            Segment::Param(name) => {
                params.push((name.clone(), parts.get(path_index)?.clone()));
                path_index += 1;
            }
            Segment::Wildcard(name) => {
                let value = parts[path_index..].join("/");
                params.push((name.clone(), value));
                path_index = parts.len();
            }
        }
    }

    if path_index == parts.len() {
        Some(params)
    } else {
        None
    }
}

fn main() {}