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
            .find_map(|(i, c)| if c == '?' || c == '#' { Some(i) } else { None })
            .unwrap_or(path.len());
        let path_head = &path[..head_end];
        let path_segments = parse_path(path_head)?;

        let mut best: Option<(usize, Vec<(String, String)>)> = None;

        for (route_id, route) in self.routes.iter().enumerate() {
            if route.segments.len() != path_segments.len() {
                continue;
            }

            let mut params = Vec::new();
            let mut matched = true;

            for (pattern_segment, value) in route.segments.iter().zip(&path_segments) {
                match pattern_segment {
                    Segment::Static(expected) if expected == value => {}
                    Segment::Static(_) => {
                        matched = false;
                        break;
                    }
                    Segment::Param(name) => params.push((name.clone(), value.clone())),
                }
            }

            if !matched {
                continue;
            }

            match &best {
                None => best = Some((route_id, params)),
                Some((best_id, _)) => {
                    if route.rank > self.routes[*best_id].rank {
                        best = Some((route_id, params));
                    }
                }
            }
        }

        match best {
            Some((route, params)) => Ok(Match { route, params }),
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
            .map(|i| start + i)
            .unwrap_or(bytes.len());

        if end == start {
            return Err(Error::InvalidPattern { at: start });
        }

        let raw = &pattern[start..end];
        if raw.as_bytes()[0] == b':' {
            let name_bytes = &raw.as_bytes()[1..];
            let valid = !name_bytes.is_empty()
                && name_bytes[0].is_ascii_lowercase()
                && name_bytes
                    .iter()
                    .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || *b == b'_');
            if !valid {
                return Err(Error::InvalidPattern { at: start });
            }
            let name = raw[1..].to_string();
            if names.iter().any(|existing| existing == &name) {
                return Err(Error::InvalidPattern { at: start });
            }
            names.push(name.clone());
            segments.push(Segment::Param(name));
        } else if raw.as_bytes()[0] == b'*' {
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
        let end = bytes[start..]
            .iter()
            .position(|&b| b == b'/')
            .map(|i| start + i)
            .unwrap_or(bytes.len());

        if end == start {
            return Err(Error::InvalidPath { at: start });
        }

        let decoded = decode_static_segment(&path[start..end], start, false)?;
        segments.push(decoded);

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

fn decode_static_segment(
    raw: &str,
    offset: usize,
    pattern: bool,
) -> Result<String, Error> {
    let bytes = raw.as_bytes();
    let mut decoded = Vec::with_capacity(bytes.len());
    let mut i = 0;

    while i < bytes.len() {
        let b = bytes[i];
        if b == b'%' {
            if i + 2 >= bytes.len() {
                return invalid(pattern, offset + i);
            }
            let Some(high) = uppercase_hex(bytes[i + 1]) else {
                return invalid(pattern, offset + i);
            };
            let Some(low) = uppercase_hex(bytes[i + 2]) else {
                return invalid(pattern, offset + i);
            };
            let value = high * 16 + low;
            if !(0x20..=0x7e).contains(&value)
                || value == b'/'
                || is_unreserved(value)
            {
                return invalid(pattern, offset + i);
            }
            decoded.push(value);
            i += 3;
        } else if is_unreserved(b) {
            decoded.push(b);
            i += 1;
        } else {
            return invalid(pattern, offset + i);
        }
    }

    Ok(String::from_utf8(decoded).expect("decoded bytes are ASCII"))
}

fn is_unreserved(b: u8) -> bool {
    b.is_ascii_alphanumeric() || matches!(b, b'.' | b'_' | b'~' | b'-')
}

fn uppercase_hex(b: u8) -> Option<u8> {
    match b {
        b'0'..=b'9' => Some(b - b'0'),
        b'A'..=b'F' => Some(b - b'A' + 10),
        _ => None,
    }
}

fn invalid<T>(pattern: bool, at: usize) -> Result<T, Error> {
    if pattern {
        Err(Error::InvalidPattern { at })
    } else {
        Err(Error::InvalidPath { at })
    }
}