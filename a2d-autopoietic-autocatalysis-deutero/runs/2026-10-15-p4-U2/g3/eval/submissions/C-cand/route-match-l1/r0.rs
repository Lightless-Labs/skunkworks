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
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct Route {
    segments: Vec<Segment>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct ParsedPath {
    segments: Vec<String>,
}

pub struct Router {
    routes: Vec<Route>,
}

impl Router {
    pub fn new() -> Self {
        Self { routes: Vec::new() }
    }

    pub fn add(&mut self, pattern: &str) -> Result<usize, Error> {
        let route = parse_pattern(pattern)?;
        let id = self.routes.len();
        self.routes.push(route);
        Ok(id)
    }

    pub fn resolve(&self, path: &str) -> Result<Match, Error> {
        let head_end = path
            .char_indices()
            .find_map(|(i, c)| (c == '?' || c == '#').then_some(i))
            .unwrap_or(path.len());
        let parsed = parse_path_head(&path[..head_end])?;

        let mut best: Option<(usize, Vec<u8>, Vec<(String, String)>)> = None;

        for (route_id, route) in self.routes.iter().enumerate() {
            if route.segments.len() != parsed.segments.len() {
                continue;
            }

            let mut rank = Vec::with_capacity(route.segments.len());
            let mut params = Vec::new();
            let mut matches = true;

            for (pattern_segment, value) in route.segments.iter().zip(&parsed.segments) {
                match pattern_segment {
                    Segment::Static(expected) => {
                        rank.push(2);
                        if expected != value {
                            matches = false;
                            break;
                        }
                    }
                    Segment::Param(name) => {
                        rank.push(1);
                        params.push((name.clone(), value.clone()));
                    }
                }
            }

            if matches {
                let replace = match &best {
                    None => true,
                    Some((_, best_rank, _)) => rank > *best_rank,
                };
                if replace {
                    best = Some((route_id, rank, params));
                }
            }
        }

        match best {
            Some((route, _, params)) => Ok(Match { route, params }),
            None => Err(Error::NotFound),
        }
    }
}

fn parse_pattern(pattern: &str) -> Result<Route, Error> {
    if !pattern.starts_with('/') {
        return Err(Error::InvalidPattern { at: 0 });
    }

    if pattern == "/" {
        return Ok(Route {
            segments: Vec::new(),
        });
    }

    let bytes = pattern.as_bytes();
    let mut segments = Vec::new();
    let mut names = Vec::<String>::new();
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

        let segment_bytes = &bytes[start..end];

        if segment_bytes[0] == b':' {
            if segment_bytes.len() < 2
                || !segment_bytes[1].is_ascii_lowercase()
                || !segment_bytes[2..]
                    .iter()
                    .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || *b == b'_')
            {
                return Err(Error::InvalidPattern { at: start });
            }
            let name = std::str::from_utf8(&segment_bytes[1..])
                .expect("capture names are ASCII")
                .to_owned();
            if names.iter().any(|existing| existing == &name) {
                return Err(Error::InvalidPattern { at: start });
            }
            names.push(name.clone());
            segments.push(Segment::Param(name));
        } else if segment_bytes[0] == b'*' {
            return Err(Error::InvalidPattern { at: start });
        } else {
            let decoded = decode_segment(segment_bytes)
                .map_err(|offset| Error::InvalidPattern { at: start + offset })?;
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

    Ok(Route { segments })
}

fn parse_path_head(path: &str) -> Result<ParsedPath, Error> {
    if !path.starts_with('/') {
        return Err(Error::InvalidPath { at: 0 });
    }

    if path == "/" {
        return Ok(ParsedPath {
            segments: Vec::new(),
        });
    }

    let bytes = path.as_bytes();
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

        let decoded = decode_segment(&bytes[start..end])
            .map_err(|offset| Error::InvalidPath { at: start + offset })?;
        segments.push(decoded);

        if end == bytes.len() {
            break;
        }
        if end + 1 == bytes.len() {
            return Err(Error::InvalidPath { at: end });
        }
        start = end + 1;
    }

    Ok(ParsedPath { segments })
}

fn decode_segment(input: &[u8]) -> Result<String, usize> {
    let mut decoded = Vec::with_capacity(input.len());
    let mut i = 0;

    while i < input.len() {
        let byte = input[i];
        if byte == b'%' {
            if i + 2 >= input.len() {
                return Err(i);
            }
            let high = uppercase_hex(input[i + 1]).ok_or(i)?;
            let low = uppercase_hex(input[i + 2]).ok_or(i)?;
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

    Ok(String::from_utf8(decoded).expect("decoded segment is ASCII"))
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

fn main() {}