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
        let rank = segments
            .iter()
            .map(|segment| match segment {
                Segment::Static(_) => 2,
                Segment::Param(_) => 1,
            })
            .collect();

        let id = self.routes.len();
        self.routes.push(Route { segments, rank });
        Ok(id)
    }

    pub fn resolve(&self, path: &str) -> Result<Match, Error> {
        let head_end = path
            .char_indices()
            .find_map(|(i, c)| (c == '?' || c == '#').then_some(i))
            .unwrap_or(path.len());
        let path_head = &path[..head_end];
        let request_segments = parse_path(path_head)?;

        let mut best: Option<(usize, Vec<u8>, Vec<(String, String)>)> = None;

        for (route_id, route) in self.routes.iter().enumerate() {
            if route.segments.len() != request_segments.len() {
                continue;
            }

            let mut params = Vec::new();
            let mut matched = true;

            for (pattern_segment, request_segment) in
                route.segments.iter().zip(request_segments.iter())
            {
                match pattern_segment {
                    Segment::Static(value) if value == request_segment => {}
                    Segment::Static(_) => {
                        matched = false;
                        break;
                    }
                    Segment::Param(name) => {
                        params.push((name.clone(), request_segment.clone()));
                    }
                }
            }

            if !matched {
                continue;
            }

            let should_replace = best
                .as_ref()
                .is_none_or(|(_, best_rank, _)| route.rank > *best_rank);

            if should_replace {
                best = Some((route_id, route.rank.clone(), params));
            }
        }

        match best {
            Some((route, _, params)) => Ok(Match { route, params }),
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
    let mut parsed = Vec::new();
    let mut names = std::collections::HashSet::new();
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

        if raw[0] == b':' {
            let name_bytes = &raw[1..];
            if !valid_capture_name(name_bytes) {
                return Err(Error::InvalidPattern { at: start });
            }
            let name = std::str::from_utf8(name_bytes)
                .expect("capture names are ASCII")
                .to_owned();
            if !names.insert(name.clone()) {
                return Err(Error::InvalidPattern { at: start });
            }
            parsed.push(Segment::Param(name));
        } else if raw[0] == b'*' {
            return Err(Error::InvalidPattern { at: start });
        } else {
            let decoded = decode_static_segment(raw)
                .map_err(|offset| Error::InvalidPattern { at: start + offset })?;
            parsed.push(Segment::Static(decoded));
        }

        if end == bytes.len() {
            break;
        }
        if end + 1 == bytes.len() {
            return Err(Error::InvalidPattern { at: end });
        }
        start = end + 1;
    }

    Ok(parsed)
}

fn parse_path(path: &str) -> Result<Vec<String>, Error> {
    if !path.starts_with('/') {
        return Err(Error::InvalidPath { at: 0 });
    }
    if path == "/" {
        return Ok(Vec::new());
    }

    let bytes = path.as_bytes();
    let mut parsed = Vec::new();
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

        let decoded = decode_static_segment(&bytes[start..end])
            .map_err(|offset| Error::InvalidPath { at: start + offset })?;
        parsed.push(decoded);

        if end == bytes.len() {
            break;
        }
        if end + 1 == bytes.len() {
            return Err(Error::InvalidPath { at: end });
        }
        start = end + 1;
    }

    Ok(parsed)
}

fn valid_capture_name(name: &[u8]) -> bool {
    if name.is_empty() || !name[0].is_ascii_lowercase() {
        return false;
    }
    name[1..]
        .iter()
        .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || *b == b'_')
}

fn decode_static_segment(raw: &[u8]) -> Result<String, usize> {
    let mut decoded = Vec::with_capacity(raw.len());
    let mut i = 0;

    while i < raw.len() {
        let byte = raw[i];
        if byte == b'%' {
            if i + 2 >= raw.len() {
                return Err(i);
            }
            let high = uppercase_hex(raw[i + 1]).ok_or(i)?;
            let low = uppercase_hex(raw[i + 2]).ok_or(i)?;
            let value = (high << 4) | low;

            if !(0x20..=0x7e).contains(&value)
                || value == b'/'
                || is_unreserved(value)
            {
                return Err(i);
            }

            decoded.push(value);
            i += 3;
        } else {
            if !is_unreserved(byte) {
                return Err(i);
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