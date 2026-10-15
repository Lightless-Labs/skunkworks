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
    Parameter(String),
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
            .bytes()
            .position(|b| b == b'?' || b == b'#')
            .unwrap_or(path.len());
        let head = &path[..head_end];
        let request_segments = parse_path(head)?;

        let mut best: Option<(&Route, Vec<(String, String)>)> = None;

        for route in &self.routes {
            if route.segments.len() != request_segments.len() {
                continue;
            }

            let mut params = Vec::new();
            let mut matched = true;

            for (pattern_segment, request_segment) in
                route.segments.iter().zip(request_segments.iter())
            {
                match pattern_segment {
                    Segment::Static(value) => {
                        if value != request_segment {
                            matched = false;
                            break;
                        }
                    }
                    Segment::Parameter(name) => {
                        params.push((name.clone(), request_segment.clone()));
                    }
                }
            }

            if !matched {
                continue;
            }

            let should_replace = match &best {
                None => true,
                Some((current, _)) => {
                    compare_rank(&route.segments, &current.segments)
                        == std::cmp::Ordering::Greater
                }
            };

            if should_replace {
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

fn compare_rank(a: &[Segment], b: &[Segment]) -> std::cmp::Ordering {
    for (left, right) in a.iter().zip(b.iter()) {
        let left_rank = match left {
            Segment::Static(_) => 2,
            Segment::Parameter(_) => 1,
        };
        let right_rank = match right {
            Segment::Static(_) => 2,
            Segment::Parameter(_) => 1,
        };
        match left_rank.cmp(&right_rank) {
            std::cmp::Ordering::Equal => {}
            other => return other,
        }
    }
    std::cmp::Ordering::Equal
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
        if bytes[start] == b'/' {
            return Err(Error::InvalidPattern { at: start });
        }

        let end = bytes[start..]
            .iter()
            .position(|&b| b == b'/')
            .map(|offset| start + offset)
            .unwrap_or(bytes.len());

        let raw = &pattern[start..end];
        if raw.starts_with(':') {
            let name = &raw[1..];
            if !valid_capture_name(name) {
                return Err(Error::InvalidPattern { at: start });
            }
            if names.iter().any(|existing| existing == name) {
                return Err(Error::InvalidPattern { at: start });
            }
            names.push(name.to_string());
            segments.push(Segment::Parameter(name.to_string()));
        } else if raw.starts_with('*') {
            return Err(Error::InvalidPattern { at: start });
        } else {
            let decoded = decode_static_segment(raw, start, true)?;
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
    let mut segments = Vec::new();
    let mut start = 1;

    while start < bytes.len() {
        if bytes[start] == b'/' {
            return Err(Error::InvalidPath { at: start });
        }

        let end = bytes[start..]
            .iter()
            .position(|&b| b == b'/')
            .map(|offset| start + offset)
            .unwrap_or(bytes.len());

        let raw = &path[start..end];
        segments.push(decode_static_segment(raw, start, false)?);

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

fn valid_capture_name(name: &str) -> bool {
    let bytes = name.as_bytes();
    if bytes.is_empty() || !bytes[0].is_ascii_lowercase() {
        return false;
    }
    bytes[1..]
        .iter()
        .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || *b == b'_')
}

fn decode_static_segment(
    raw: &str,
    base: usize,
    pattern: bool,
) -> Result<String, Error> {
    let bytes = raw.as_bytes();
    let mut decoded = Vec::with_capacity(bytes.len());
    let mut i = 0;

    while i < bytes.len() {
        let b = bytes[i];
        if b == b'%' {
            if i + 2 >= bytes.len() {
                return invalid_segment(pattern, base + i);
            }
            let Some(high) = uppercase_hex(bytes[i + 1]) else {
                return invalid_segment(pattern, base + i);
            };
            let Some(low) = uppercase_hex(bytes[i + 2]) else {
                return invalid_segment(pattern, base + i);
            };
            let value = (high << 4) | low;
            if !(0x20..=0x7e).contains(&value)
                || value == b'/'
                || is_unreserved(value)
            {
                return invalid_segment(pattern, base + i);
            }
            decoded.push(value);
            i += 3;
        } else if is_unreserved(b) {
            decoded.push(b);
            i += 1;
        } else {
            return invalid_segment(pattern, base + i);
        }
    }

    // Every accepted raw byte and decoded escape is ASCII.
    Ok(String::from_utf8(decoded).expect("validated ASCII segment"))
}

fn uppercase_hex(b: u8) -> Option<u8> {
    match b {
        b'0'..=b'9' => Some(b - b'0'),
        b'A'..=b'F' => Some(b - b'A' + 10),
        _ => None,
    }
}

fn is_unreserved(b: u8) -> bool {
    b.is_ascii_alphanumeric() || matches!(b, b'.' | b'_' | b'~' | b'-')
}

fn invalid_segment<T>(pattern: bool, at: usize) -> Result<T, Error> {
    if pattern {
        Err(Error::InvalidPattern { at })
    } else {
        Err(Error::InvalidPath { at })
    }
}

fn main() {}