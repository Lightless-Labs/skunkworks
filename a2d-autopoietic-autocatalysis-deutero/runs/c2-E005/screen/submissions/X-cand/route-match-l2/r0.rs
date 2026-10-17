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
        let parsed = parse_pattern(pattern)?;
        let id = self.routes.len();
        let rank = route_rank(&parsed);
        self.routes.push(Route {
            segments: parsed,
            id,
            rank,
        });
        Ok(id)
    }

    pub fn resolve(&self, path: &str) -> Result<Match, Error> {
        let head_end = path
            .as_bytes()
            .iter()
            .position(|&b| b == b'?' || b == b'#')
            .unwrap_or(path.len());
        let head = &path[..head_end];
        let request = parse_path(head)?;

        let mut best: Option<(usize, Match)> = None;
        for route in &self.routes {
            if let Some(params) = match_route(&route.segments, &request) {
                let candidate = Match {
                    route: route.id,
                    params,
                };
                let replace = match &best {
                    None => true,
                    Some((best_index, _)) => {
                        route.rank > self.routes[*best_index].rank
                    }
                };
                if replace {
                    let index = self.routes.iter().position(|r| r.id == route.id).unwrap();
                    best = Some((index, candidate));
                }
            }
        }

        best.map(|(_, matched)| matched).ok_or(Error::NotFound)
    }
}

impl Default for Router {
    fn default() -> Self {
        Self::new()
    }
}

fn parse_pattern(pattern: &str) -> Result<Vec<Segment>, Error> {
    let bytes = pattern.as_bytes();
    if bytes.first() != Some(&b'/') {
        return Err(Error::InvalidPattern { at: 0 });
    }
    if pattern == "/" {
        return Ok(Vec::new());
    }

    let mut end = bytes.len();
    let trailing = bytes.last() == Some(&b'/');
    if trailing {
        end -= 1;
    }

    let mut segments = Vec::new();
    let mut names = std::collections::HashSet::new();
    let mut start = 1;

    while start < end {
        let segment_end = bytes[start..end]
            .iter()
            .position(|&b| b == b'/')
            .map(|offset| start + offset)
            .unwrap_or(end);

        if segment_end == start {
            return Err(Error::InvalidPattern { at: start });
        }

        let raw = &pattern[start..segment_end];
        let first = bytes[start];

        if first == b':' || first == b'*' {
            let name_bytes = &bytes[start + 1..segment_end];
            if !valid_name(name_bytes) {
                return Err(Error::InvalidPattern { at: start });
            }
            let name = std::str::from_utf8(name_bytes)
                .expect("capture names are ASCII")
                .to_owned();
            if !names.insert(name.clone()) {
                return Err(Error::InvalidPattern { at: start });
            }
            if first == b'*' {
                if segment_end != end {
                    return Err(Error::InvalidPattern { at: start });
                }
                segments.push(Segment::Wildcard(name));
            } else {
                segments.push(Segment::Param(name));
            }
        } else {
            let decoded = decode_static(raw, start).map_err(|at| Error::InvalidPattern { at })?;
            segments.push(Segment::Static(decoded));
        }

        if segment_end == end {
            break;
        }
        start = segment_end + 1;
    }

    if trailing {
        return Err(Error::InvalidPattern { at: bytes.len() - 1 });
    }

    Ok(segments)
}

fn parse_path(path: &str) -> Result<Vec<String>, Error> {
    let bytes = path.as_bytes();
    if bytes.first() != Some(&b'/') {
        return Err(Error::InvalidPath { at: 0 });
    }
    if path == "/" {
        return Ok(Vec::new());
    }

    let mut end = bytes.len();
    let trailing = bytes.last() == Some(&b'/');
    if trailing {
        end -= 1;
    }

    let mut segments = Vec::new();
    let mut start = 1;

    while start < end {
        let segment_end = bytes[start..end]
            .iter()
            .position(|&b| b == b'/')
            .map(|offset| start + offset)
            .unwrap_or(end);

        if segment_end == start {
            return Err(Error::InvalidPath { at: start });
        }

        let raw = &path[start..segment_end];
        let decoded = decode_static(raw, start).map_err(|at| Error::InvalidPath { at })?;
        segments.push(decoded);

        if segment_end == end {
            break;
        }
        start = segment_end + 1;
    }

    if trailing {
        return Err(Error::InvalidPath { at: bytes.len() - 1 });
    }

    Ok(segments)
}

fn valid_name(bytes: &[u8]) -> bool {
    if bytes.is_empty() || !bytes[0].is_ascii_lowercase() {
        return false;
    }
    bytes[1..]
        .iter()
        .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || *b == b'_')
}

fn decode_static(raw: &str, base: usize) -> Result<String, usize> {
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

fn route_rank(segments: &[Segment]) -> Vec<u8> {
    let mut rank = Vec::with_capacity(segments.len() + 1);
    for segment in segments {
        rank.push(match segment {
            Segment::Static(_) => 2,
            Segment::Param(_) => 1,
            Segment::Wildcard(_) => 0,
        });
        if matches!(segment, Segment::Wildcard(_)) {
            return rank;
        }
    }
    rank.push(3);
    rank
}

fn match_route(segments: &[Segment], request: &[String]) -> Option<Vec<(String, String)>> {
    let mut params = Vec::new();
    let mut request_index = 0;

    for segment in segments {
        match segment {
            Segment::Static(expected) => {
                if request.get(request_index)? != expected {
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
            }
        }
    }

    if request_index == request.len() {
        Some(params)
    } else {
        None
    }
}