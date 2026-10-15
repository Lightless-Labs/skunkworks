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

#[derive(Debug, Clone, PartialEq, Eq)]
enum ParsedSegment {
    Static(String),
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
        self.routes.push(Route { segments, rank });
        Ok(id)
    }

    pub fn resolve(&self, path: &str) -> Result<Match, Error> {
        let head_end = path
            .char_indices()
            .find_map(|(i, c)| (c == '?' || c == '#').then_some(i))
            .unwrap_or(path.len());
        let head = &path[..head_end];
        let request_segments = parse_path(head)?;

        let mut best: Option<(usize, Vec<(String, String)>)> = None;

        for (route_id, route) in self.routes.iter().enumerate() {
            if let Some(params) = match_route(&route.segments, &request_segments) {
                let replace = match &best {
                    None => true,
                    Some((best_id, _)) => {
                        route.rank > self.routes[*best_id].rank
                            || (route.rank == self.routes[*best_id].rank
                                && route_id < *best_id)
                    }
                };
                if replace {
                    best = Some((route_id, params));
                }
            }
        }

        match best {
            Some((route, params)) => Ok(Match { route, params }),
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
    let mut end = bytes.len();
    let trailing_slash = bytes[end - 1] == b'/';
    if trailing_slash {
        end -= 1;
    }

    let mut segments = Vec::new();
    let mut names = std::collections::HashSet::new();
    let mut start = 1;

    while start < end {
        let segment_end = bytes[start..end]
            .iter()
            .position(|&b| b == b'/')
            .map(|i| start + i)
            .unwrap_or(end);

        if segment_end == start {
            return Err(Error::InvalidPattern { at: start });
        }

        let raw = &pattern[start..segment_end];
        let first = bytes[start];

        if first == b':' || first == b'*' {
            let name = &raw[1..];
            if !valid_capture_name(name) || !names.insert(name.to_owned()) {
                return Err(Error::InvalidPattern { at: start });
            }
            if first == b'*' && segment_end != end {
                return Err(Error::InvalidPattern { at: start });
            }
            segments.push(if first == b':' {
                Segment::Param(name.to_owned())
            } else {
                Segment::Wildcard(name.to_owned())
            });
        } else {
            let decoded = decode_static_segment(raw, start)
                .map_err(|at| Error::InvalidPattern { at })?;
            segments.push(Segment::Static(decoded));
        }

        start = segment_end + 1;
    }

    if trailing_slash {
        return Err(Error::InvalidPattern { at: bytes.len() - 1 });
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
    let mut end = bytes.len();
    let trailing_slash = bytes[end - 1] == b'/';
    if trailing_slash {
        end -= 1;
    }

    let mut segments = Vec::new();
    let mut start = 1;

    while start < end {
        let segment_end = bytes[start..end]
            .iter()
            .position(|&b| b == b'/')
            .map(|i| start + i)
            .unwrap_or(end);

        if segment_end == start {
            return Err(Error::InvalidPath { at: start });
        }

        let raw = &path[start..segment_end];
        let decoded =
            decode_static_segment(raw, start).map_err(|at| Error::InvalidPath { at })?;
        segments.push(decoded);

        start = segment_end + 1;
    }

    if trailing_slash {
        return Err(Error::InvalidPath { at: bytes.len() - 1 });
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

fn decode_static_segment(raw: &str, base: usize) -> Result<String, usize> {
    let bytes = raw.as_bytes();
    let mut decoded = Vec::with_capacity(bytes.len());
    let mut i = 0;

    while i < bytes.len() {
        let byte = bytes[i];
        if byte == b'%' {
            if i + 2 >= bytes.len() {
                return Err(base + i);
            }
            let hi = uppercase_hex(bytes[i + 1]).ok_or(base + i)?;
            let lo = uppercase_hex(bytes[i + 2]).ok_or(base + i)?;
            let value = (hi << 4) | lo;

            if !(0x20..=0x7e).contains(&value)
                || value == b'/'
                || is_unreserved(value)
            {
                return Err(base + i);
            }

            decoded.push(value);
            i += 3;
        } else {
            if !is_unreserved(byte) {
                return Err(base + i);
            }
            decoded.push(byte);
            i += 1;
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

fn match_route(pattern: &[Segment], request: &[String]) -> Option<Vec<(String, String)>> {
    let mut params = Vec::new();
    let mut request_index = 0;

    for segment in pattern {
        match segment {
            Segment::Static(value) => {
                if request.get(request_index)? != value {
                    return None;
                }
                request_index += 1;
            }
            Segment::Param(name) => {
                let value = request.get(request_index)?;
                params.push((name.clone(), value.clone()));
                request_index += 1;
            }
            Segment::Wildcard(name) => {
                let value = request[request_index..].join("/");
                params.push((name.clone(), value));
                request_index = request.len();
                break;
            }
        }
    }

    if request_index == request.len() {
        Some(params)
    } else {
        None
    }
}

fn main() {}