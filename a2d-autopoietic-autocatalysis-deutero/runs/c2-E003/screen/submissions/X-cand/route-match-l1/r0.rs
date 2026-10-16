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
}

#[derive(Debug, Clone)]
struct Route {
    segments: Vec<Segment>,
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
        self.routes.push(Route { segments, id });
        Ok(id)
    }

    pub fn resolve(&self, path: &str) -> Result<Match, Error> {
        let head_end = path
            .char_indices()
            .find_map(|(i, c)| if c == '?' || c == '#' { Some(i) } else { None })
            .unwrap_or(path.len());
        let head = &path[..head_end];
        let segments = parse_path(head)?;

        let mut best: Option<(&Route, Vec<(String, String)>)> = None;
        let mut best_rank: Vec<u8> = Vec::new();

        for route in &self.routes {
            if route.segments.len() != segments.len() {
                continue;
            }

            let mut rank = Vec::with_capacity(segments.len());
            let mut params = Vec::new();
            let mut matched = true;

            for (pattern_segment, request_segment) in
                route.segments.iter().zip(segments.iter())
            {
                match pattern_segment {
                    Segment::Static(value) => {
                        rank.push(2);
                        if value != request_segment {
                            matched = false;
                            break;
                        }
                    }
                    Segment::Param(name) => {
                        rank.push(1);
                        params.push((name.clone(), request_segment.clone()));
                    }
                }
            }

            if matched && (best.is_none() || rank > best_rank) {
                best_rank = rank;
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

    let mut result = Vec::new();
    let mut names = std::collections::HashSet::new();
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

        let segment = &pattern[start..end];
        let first = bytes[start];

        if first == b'*' {
            return Err(Error::InvalidPattern { at: start });
        }

        if first == b':' {
            let name_bytes = &bytes[start + 1..end];
            let valid = !name_bytes.is_empty()
                && name_bytes[0].is_ascii_lowercase()
                && name_bytes
                    .iter()
                    .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || *b == b'_');
            if !valid {
                return Err(Error::InvalidPattern { at: start });
            }
            let name = &segment[1..];
            if !names.insert(name.to_owned()) {
                return Err(Error::InvalidPattern { at: start });
            }
            result.push(Segment::Param(name.to_owned()));
        } else {
            result.push(Segment::Static(decode_static_segment(
                bytes, start, end, true,
            )?));
        }

        if end == bytes.len() {
            break;
        }
        start = end + 1;
        if start == bytes.len() {
            return Err(Error::InvalidPattern { at: end });
        }
    }

    Ok(result)
}

fn parse_path(path: &str) -> Result<Vec<String>, Error> {
    if !path.starts_with('/') {
        return Err(Error::InvalidPath { at: 0 });
    }
    if path == "/" {
        return Ok(Vec::new());
    }

    let bytes = path.as_bytes();
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

        result.push(decode_static_segment(bytes, start, end, false)?);

        if end == bytes.len() {
            break;
        }
        start = end + 1;
        if start == bytes.len() {
            return Err(Error::InvalidPath { at: end });
        }
    }

    Ok(result)
}

fn decode_static_segment(
    bytes: &[u8],
    start: usize,
    end: usize,
    pattern: bool,
) -> Result<String, Error> {
    let mut decoded = Vec::with_capacity(end - start);
    let mut i = start;

    while i < end {
        let byte = bytes[i];
        if byte == b'%' {
            if i + 2 >= end {
                return invalid_segment(pattern, i);
            }
            let hi = upper_hex(bytes[i + 1]);
            let lo = upper_hex(bytes[i + 2]);
            let (Some(hi), Some(lo)) = (hi, lo) else {
                return invalid_segment(pattern, i);
            };
            let value = (hi << 4) | lo;
            if !(0x20..=0x7e).contains(&value)
                || value == b'/'
                || is_unreserved(value)
            {
                return invalid_segment(pattern, i);
            }
            decoded.push(value);
            i += 3;
        } else if is_unreserved(byte) {
            decoded.push(byte);
            i += 1;
        } else {
            return invalid_segment(pattern, i);
        }
    }

    // All accepted raw and decoded bytes are ASCII.
    Ok(String::from_utf8(decoded).expect("validated segment is ASCII"))
}

fn invalid_segment<T>(pattern: bool, at: usize) -> Result<T, Error> {
    if pattern {
        Err(Error::InvalidPattern { at })
    } else {
        Err(Error::InvalidPath { at })
    }
}

fn is_unreserved(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'_' | b'~' | b'-')
}

fn upper_hex(byte: u8) -> Option<u8> {
    match byte {
        b'0'..=b'9' => Some(byte - b'0'),
        b'A'..=b'F' => Some(byte - b'A' + 10),
        _ => None,
    }
}