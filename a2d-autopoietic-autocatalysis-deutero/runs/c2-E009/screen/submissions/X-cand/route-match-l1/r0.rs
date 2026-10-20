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
        let parsed = parse_pattern(pattern)?;
        let id = self.routes.len();
        self.routes.push(Route {
            id,
            segments: parsed.0,
            rank: parsed.1,
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
        let request_segments = parse_path(head)?;

        let mut best: Option<(usize, Vec<(String, String)>)> = None;
        let mut best_rank: Option<&[u8]> = None;

        for route in &self.routes {
            if route.segments.len() != request_segments.len() {
                continue;
            }

            let mut params = Vec::new();
            let mut matched = true;
            for (pattern_segment, value) in route.segments.iter().zip(&request_segments) {
                match pattern_segment {
                    Segment::Static(expected) if expected == value => {}
                    Segment::Static(_) => {
                        matched = false;
                        break;
                    }
                    Segment::Param(name) => params.push((name.clone(), value.clone())),
                }
            }

            if matched
                && best_rank
                    .map(|rank| route.rank.as_slice() > rank)
                    .unwrap_or(true)
            {
                best = Some((route.id, params));
                best_rank = Some(&route.rank);
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

fn parse_pattern(pattern: &str) -> Result<(Vec<Segment>, Vec<u8>), Error> {
    let bytes = pattern.as_bytes();
    if bytes.first() != Some(&b'/') {
        return Err(Error::InvalidPattern { at: 0 });
    }
    if pattern == "/" {
        return Ok((Vec::new(), Vec::new()));
    }

    let mut segments = Vec::new();
    let mut rank = Vec::new();
    let mut names: Vec<String> = Vec::new();

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

        let segment = &bytes[start..end];
        if segment[0] == b':' {
            let name_bytes = &segment[1..];
            let valid = name_bytes.first().is_some_and(u8::is_ascii_lowercase)
                && name_bytes
                    .iter()
                    .skip(1)
                    .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || *b == b'_');
            if !valid {
                return Err(Error::InvalidPattern { at: start });
            }

            let name = std::str::from_utf8(name_bytes)
                .expect("capture names are ASCII")
                .to_owned();
            if names.iter().any(|existing| existing == &name) {
                return Err(Error::InvalidPattern { at: start });
            }
            names.push(name.clone());
            segments.push(Segment::Param(name));
            rank.push(1);
        } else if segment[0] == b'*' {
            return Err(Error::InvalidPattern { at: start });
        } else {
            let decoded = decode_static_segment(segment, start)
                .map_err(|at| Error::InvalidPattern { at })?;
            segments.push(Segment::Static(decoded));
            rank.push(2);
        }

        if end == bytes.len() {
            break;
        }
        start = end + 1;
        if start == bytes.len() {
            return Err(Error::InvalidPattern { at: bytes.len() - 1 });
        }
    }

    Ok((segments, rank))
}

fn parse_path(path: &str) -> Result<Vec<String>, Error> {
    let bytes = path.as_bytes();
    if bytes.first() != Some(&b'/') {
        return Err(Error::InvalidPath { at: 0 });
    }
    if path == "/" {
        return Ok(Vec::new());
    }

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

        let decoded = decode_static_segment(&bytes[start..end], start)
            .map_err(|at| Error::InvalidPath { at })?;
        segments.push(decoded);

        if end == bytes.len() {
            break;
        }
        start = end + 1;
        if start == bytes.len() {
            return Err(Error::InvalidPath { at: bytes.len() - 1 });
        }
    }

    Ok(segments)
}

fn decode_static_segment(segment: &[u8], base: usize) -> Result<String, usize> {
    let mut decoded = Vec::new();
    let mut i = 0;

    while i < segment.len() {
        let byte = segment[i];
        if byte == b'%' {
            if i + 2 >= segment.len() {
                return Err(base + i);
            }
            let hi = uppercase_hex(segment[i + 1]).ok_or(base + i)?;
            let lo = uppercase_hex(segment[i + 2]).ok_or(base + i)?;
            let value = (hi << 4) | lo;
            if !(0x20..=0x7e).contains(&value)
                || value == b'/'
                || is_unreserved(value)
            {
                return Err(base + i);
            }
            decoded.push(value);
            i += 3;
        } else if is_unreserved(byte) {
            decoded.push(byte);
            i += 1;
        } else {
            return Err(base + i);
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