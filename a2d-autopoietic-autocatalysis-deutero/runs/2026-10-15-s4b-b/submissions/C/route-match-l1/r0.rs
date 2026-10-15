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
        let segments = parse_pattern(pattern)?;
        let id = self.routes.len();
        let rank = segments
            .iter()
            .map(|s| match s {
                Segment::Static(_) => 2,
                Segment::Param(_) => 1,
            })
            .collect();
        self.routes.push(Route { segments, rank, id });
        Ok(id)
    }

    pub fn resolve(&self, path: &str) -> Result<Match, Error> {
        let head_end = path
            .char_indices()
            .find(|(_, c)| *c == '?' || *c == '#')
            .map(|(i, _)| i)
            .unwrap_or(path.len());
        let head = &path[..head_end];
        let parts = parse_path(head)?;

        let mut best: Option<(usize, Vec<(String, String)>)> = None;
        for route in &self.routes {
            if route.segments.len() != parts.len() {
                continue;
            }
            let mut params = Vec::new();
            let mut matched = true;
            for (pattern_seg, request_seg) in route.segments.iter().zip(parts.iter()) {
                match pattern_seg {
                    Segment::Static(value) if value == request_seg => {}
                    Segment::Param(name) => params.push((name.clone(), request_seg.clone())),
                    _ => {
                        matched = false;
                        break;
                    }
                }
            }
            if !matched {
                continue;
            }
            let replace = match best.as_ref() {
                None => true,
                Some((best_id, _)) => {
                    let best_route = &self.routes[*best_id];
                    route.rank > best_route.rank
                }
            };
            if replace {
                best = Some((route.id, params));
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

    let mut result = Vec::new();
    let mut names = std::collections::HashSet::new();
    let bytes = input.as_bytes();
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
            if !valid_name(name) || !names.insert(name.to_string()) {
                return Err(Error::InvalidPattern { at: start });
            }
            result.push(Segment::Param(name.to_string()));
        } else if raw.starts_with('*') {
            return Err(Error::InvalidPattern { at: start });
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
            .map(|n| start + n)
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

fn decode_segment(segment: &str, offset: usize) -> Result<String, usize> {
    let bytes = segment.as_bytes();
    let mut decoded = Vec::with_capacity(bytes.len());
    let mut i = 0;

    while i < bytes.len() {
        let b = bytes[i];
        if b == b'%' {
            if i + 2 >= bytes.len() {
                return Err(offset + i);
            }
            let hi = upper_hex(bytes[i + 1]).ok_or(offset + i)?;
            let lo = upper_hex(bytes[i + 2]).ok_or(offset + i)?;
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

    Ok(String::from_utf8(decoded).expect("decoded segment is ASCII"))
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

fn main() {}