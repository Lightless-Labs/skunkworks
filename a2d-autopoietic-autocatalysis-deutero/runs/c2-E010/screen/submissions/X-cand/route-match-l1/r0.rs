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
}

#[derive(Clone)]
struct Route {
    id: usize,
    segments: Vec<Segment>,
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
        self.routes.push(Route { id, segments });
        Ok(id)
    }

    pub fn resolve(&self, path: &str) -> Result<Match, Error> {
        let head_end = path
            .char_indices()
            .find_map(|(i, c)| if c == '?' || c == '#' { Some(i) } else { None })
            .unwrap_or(path.len());
        let head = &path[..head_end];
        let request = parse_path(head)?;

        let mut best: Option<(Vec<u8>, Match)> = None;

        for route in &self.routes {
            if route.segments.len() != request.len() {
                continue;
            }

            let mut rank = Vec::with_capacity(route.segments.len());
            let mut params = Vec::new();
            let mut matches = true;

            for (pattern_seg, value) in route.segments.iter().zip(&request) {
                match pattern_seg {
                    Segment::Static(expected) => {
                        if expected != value {
                            matches = false;
                            break;
                        }
                        rank.push(2);
                    }
                    Segment::Param(name) => {
                        rank.push(1);
                        params.push((name.clone(), value.clone()));
                    }
                }
            }

            if matches {
                let candidate = Match {
                    route: route.id,
                    params,
                };
                let replace = match &best {
                    None => true,
                    Some((best_rank, best_match)) => {
                        rank > *best_rank
                            || (rank == *best_rank && candidate.route < best_match.route)
                    }
                };
                if replace {
                    best = Some((rank, candidate));
                }
            }
        }

        best.map(|(_, matched)| matched).ok_or(Error::NotFound)
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
    let mut parts = Vec::new();
    let mut start = 1;

    while start < bytes.len() {
        let end = bytes[start..]
            .iter()
            .position(|&b| b == b'/')
            .map(|n| start + n)
            .unwrap_or(bytes.len());

        if end == start {
            return Err(Error::InvalidPattern { at: start });
        }

        let raw = &input[start..end];
        if raw.starts_with(':') {
            let name = &raw[1..];
            if !valid_capture_name(name) {
                return Err(Error::InvalidPattern { at: start });
            }
            if parts.iter().any(|seg| matches!(seg, Segment::Param(existing) if existing == name)) {
                return Err(Error::InvalidPattern { at: start });
            }
            parts.push(Segment::Param(name.to_owned()));
        } else if raw.starts_with('*') {
            return Err(Error::InvalidPattern { at: start });
        } else {
            let decoded = decode_static_segment(raw, start).map_err(|at| Error::InvalidPattern { at })?;
            parts.push(Segment::Static(decoded));
        }

        if end == bytes.len() {
            break;
        }
        start = end + 1;
        if start == bytes.len() {
            return Err(Error::InvalidPattern { at: bytes.len() - 1 });
        }
    }

    Ok(parts)
}

fn parse_path(input: &str) -> Result<Vec<String>, Error> {
    if !input.starts_with('/') {
        return Err(Error::InvalidPath { at: 0 });
    }
    if input == "/" {
        return Ok(Vec::new());
    }

    let bytes = input.as_bytes();
    let mut parts = Vec::new();
    let mut start = 1;

    while start < bytes.len() {
        let end = bytes[start..]
            .iter()
            .position(|&b| b == b'/')
            .map(|n| start + n)
            .unwrap_or(bytes.len());

        if end == start {
            return Err(Error::InvalidPath { at: start });
        }

        let decoded = decode_static_segment(&input[start..end], start)
            .map_err(|at| Error::InvalidPath { at })?;
        parts.push(decoded);

        if end == bytes.len() {
            break;
        }
        start = end + 1;
        if start == bytes.len() {
            return Err(Error::InvalidPath { at: bytes.len() - 1 });
        }
    }

    Ok(parts)
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

fn is_unreserved(b: u8) -> bool {
    b.is_ascii_alphanumeric() || matches!(b, b'.' | b'_' | b'~' | b'-')
}

fn hex_value(b: u8) -> Option<u8> {
    match b {
        b'0'..=b'9' => Some(b - b'0'),
        b'A'..=b'F' => Some(b - b'A' + 10),
        _ => None,
    }
}

fn decode_static_segment(raw: &str, base: usize) -> Result<String, usize> {
    let bytes = raw.as_bytes();
    let mut decoded = Vec::with_capacity(bytes.len());
    let mut i = 0;

    while i < bytes.len() {
        let b = bytes[i];
        if b == b'%' {
            if i + 2 >= bytes.len() {
                return Err(base + i);
            }
            let hi = hex_value(bytes[i + 1]).ok_or(base + i)?;
            let lo = hex_value(bytes[i + 2]).ok_or(base + i)?;
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
            if !is_unreserved(b) {
                return Err(base + i);
            }
            decoded.push(b);
            i += 1;
        }
    }

    Ok(String::from_utf8(decoded).expect("decoded bytes are ASCII"))
}
