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

        let has_wildcard = segments
            .iter()
            .any(|segment| matches!(segment, Segment::Wildcard(_)));
        let mut rank = Vec::with_capacity(segments.len() + 1);

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
        self.routes.push(Route { id, segments, rank });
        Ok(id)
    }

    pub fn resolve(&self, path: &str) -> Result<Match, Error> {
        let head_end = path
            .bytes()
            .position(|byte| byte == b'?' || byte == b'#')
            .unwrap_or(path.len());
        let request_segments = parse_path(&path[..head_end])?;

        let mut best: Option<(&Route, Vec<(String, String)>)> = None;
        for route in &self.routes {
            if let Some(params) = match_route(route, &request_segments) {
                let replace = best
                    .as_ref()
                    .map(|(current, _)| route.rank > current.rank)
                    .unwrap_or(true);
                if replace {
                    best = Some((route, params));
                }
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
    let bytes = pattern.as_bytes();
    if bytes.first() != Some(&b'/') {
        return Err(Error::InvalidPattern { at: 0 });
    }
    if pattern == "/" {
        return Ok(Vec::new());
    }

    let mut segments = Vec::new();
    let mut names = Vec::<String>::new();
    let mut start = 1;

    loop {
        let end = bytes[start..]
            .iter()
            .position(|&byte| byte == b'/')
            .map(|offset| start + offset)
            .unwrap_or(bytes.len());

        if end == start {
            return Err(Error::InvalidPattern { at: start });
        }

        let raw = &pattern[start..end];
        match bytes[start] {
            b':' | b'*' => {
                let wildcard = bytes[start] == b'*';
                let name = &raw[1..];

                if !valid_name(name) || names.iter().any(|existing| existing == name) {
                    return Err(Error::InvalidPattern { at: start });
                }
                if wildcard && end != bytes.len() {
                    return Err(Error::InvalidPattern { at: start });
                }

                names.push(name.to_owned());
                if wildcard {
                    segments.push(Segment::Wildcard(name.to_owned()));
                } else {
                    segments.push(Segment::Param(name.to_owned()));
                }
            }
            _ => segments.push(Segment::Static(decode_segment(
                raw,
                start,
                ErrorKind::Pattern,
            )?)),
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
    let bytes = path.as_bytes();
    if bytes.first() != Some(&b'/') {
        return Err(Error::InvalidPath { at: 0 });
    }
    if path == "/" {
        return Ok(Vec::new());
    }

    let mut segments = Vec::new();
    let mut start = 1;

    loop {
        let end = bytes[start..]
            .iter()
            .position(|&byte| byte == b'/')
            .map(|offset| start + offset)
            .unwrap_or(bytes.len());

        if end == start {
            return Err(Error::InvalidPath { at: start });
        }

        segments.push(decode_segment(
            &path[start..end],
            start,
            ErrorKind::Path,
        )?);

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

#[derive(Clone, Copy)]
enum ErrorKind {
    Pattern,
    Path,
}

impl ErrorKind {
    fn at(self, at: usize) -> Error {
        match self {
            Self::Pattern => Error::InvalidPattern { at },
            Self::Path => Error::InvalidPath { at },
        }
    }
}

fn decode_segment(raw: &str, base: usize, kind: ErrorKind) -> Result<String, Error> {
    let bytes = raw.as_bytes();
    let mut decoded = Vec::with_capacity(bytes.len());
    let mut index = 0;

    while index < bytes.len() {
        let byte = bytes[index];
        if byte == b'%' {
            if index + 2 >= bytes.len() {
                return Err(kind.at(base + index));
            }
            let high = uppercase_hex(bytes[index + 1]).ok_or_else(|| kind.at(base + index))?;
            let low = uppercase_hex(bytes[index + 2]).ok_or_else(|| kind.at(base + index))?;
            let value = high * 16 + low;

            if !(0x20..=0x7e).contains(&value)
                || value == b'/'
                || is_unreserved(value)
            {
                return Err(kind.at(base + index));
            }

            decoded.push(value);
            index += 3;
        } else if is_unreserved(byte) {
            decoded.push(byte);
            index += 1;
        } else {
            return Err(kind.at(base + index));
        }
    }

    Ok(String::from_utf8(decoded).expect("accepted segment bytes are ASCII"))
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

fn valid_name(name: &str) -> bool {
    let bytes = name.as_bytes();
    if bytes.is_empty() || !bytes[0].is_ascii_lowercase() {
        return false;
    }
    bytes[1..]
        .iter()
        .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || *byte == b'_')
}

fn match_route(route: &Route, path: &[String]) -> Option<Vec<(String, String)>> {
    let mut params = Vec::new();
    let mut path_index = 0;

    for segment in &route.segments {
        match segment {
            Segment::Static(value) => {
                if path.get(path_index)? != value {
                    return None;
                }
                path_index += 1;
            }
            Segment::Param(name) => {
                params.push((name.clone(), path.get(path_index)?.clone()));
                path_index += 1;
            }
            Segment::Wildcard(name) => {
                params.push((name.clone(), path[path_index..].join("/")));
                return Some(params);
            }
        }
    }

    if path_index == path.len() {
        Some(params)
    } else {
        None
    }
}