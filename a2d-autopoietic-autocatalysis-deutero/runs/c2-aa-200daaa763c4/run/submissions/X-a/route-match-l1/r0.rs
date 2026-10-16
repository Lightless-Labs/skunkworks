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
    id: usize,
    segments: Vec<Segment>,
    rank: Vec<u8>,
}

#[derive(Debug, Clone)]
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
        let rank = parsed
            .iter()
            .map(|segment| match segment {
                Segment::Static(_) => 2,
                Segment::Param(_) => 1,
            })
            .collect();

        self.routes.push(Route {
            id,
            segments: parsed,
            rank,
        });
        Ok(id)
    }

    pub fn resolve(&self, path: &str) -> Result<Match, Error> {
        let head_end = path
            .char_indices()
            .find_map(|(index, ch)| (ch == '?' || ch == '#').then_some(index))
            .unwrap_or(path.len());
        let parsed_path = parse_path_head(&path[..head_end])?;

        let mut best: Option<(&Route, Vec<(String, String)>)> = None;

        for route in &self.routes {
            if route.segments.len() != parsed_path.len() {
                continue;
            }

            let mut params = Vec::new();
            let mut matches = true;

            for (pattern_segment, path_segment) in route.segments.iter().zip(&parsed_path) {
                match pattern_segment {
                    Segment::Static(value) if value == path_segment => {}
                    Segment::Static(_) => {
                        matches = false;
                        break;
                    }
                    Segment::Param(name) => params.push((name.clone(), path_segment.clone())),
                }
            }

            if !matches {
                continue;
            }

            let should_replace = match &best {
                None => true,
                Some((best_route, _)) => route.rank > best_route.rank,
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

fn parse_pattern(pattern: &str) -> Result<Vec<Segment>, Error> {
    if !pattern.starts_with('/') {
        return Err(Error::InvalidPattern { at: 0 });
    }

    if pattern == "/" {
        return Ok(Vec::new());
    }

    let mut segments = Vec::new();
    let mut names = std::collections::HashSet::new();
    let bytes = pattern.as_bytes();
    let mut start = 1;

    while start < bytes.len() {
        let end = bytes[start..]
            .iter()
            .position(|&byte| byte == b'/')
            .map(|offset| start + offset)
            .unwrap_or(bytes.len());

        if end == start {
            return Err(Error::InvalidPattern { at: start });
        }

        let segment = &pattern[start..end];
        let first = bytes[start];

        if first == b':' {
            let name = &segment[1..];
            if !valid_capture_name(name) || !names.insert(name.to_owned()) {
                return Err(Error::InvalidPattern { at: start });
            }
            segments.push(Segment::Param(name.to_owned()));
        } else if first == b'*' {
            return Err(Error::InvalidPattern { at: start });
        } else {
            let decoded = decode_static_segment(segment, start)
                .map_err(|at| Error::InvalidPattern { at })?;
            segments.push(Segment::Static(decoded));
        }

        if end == bytes.len() {
            break;
        }
        start = end + 1;
        if start == bytes.len() {
            return Err(Error::InvalidPattern { at: bytes.len() - 1 });
        }
    }

    Ok(segments)
}

fn parse_path_head(path: &str) -> Result<Vec<String>, Error> {
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
            .position(|&byte| byte == b'/')
            .map(|offset| start + offset)
            .unwrap_or(bytes.len());

        if end == start {
            return Err(Error::InvalidPath { at: start });
        }

        let decoded = decode_static_segment(&path[start..end], start)
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

fn valid_capture_name(name: &str) -> bool {
    let bytes = name.as_bytes();
    if bytes.is_empty() || !bytes[0].is_ascii_lowercase() {
        return false;
    }
    bytes[1..]
        .iter()
        .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || *byte == b'_')
}

fn decode_static_segment(segment: &str, base: usize) -> Result<String, usize> {
    let bytes = segment.as_bytes();
    let mut decoded = Vec::with_capacity(bytes.len());
    let mut index = 0;

    while index < bytes.len() {
        let byte = bytes[index];
        if byte == b'%' {
            if index + 2 >= bytes.len() {
                return Err(base + index);
            }

            let high = hex_value(bytes[index + 1]).ok_or(base + index)?;
            let low = hex_value(bytes[index + 2]).ok_or(base + index)?;
            let value = (high << 4) | low;

            if !(0x20..=0x7e).contains(&value)
                || value == b'/'
                || is_unreserved(value)
            {
                return Err(base + index);
            }

            decoded.push(value);
            index += 3;
        } else {
            if !is_unreserved(byte) {
                return Err(base + index);
            }
            decoded.push(byte);
            index += 1;
        }
    }

    Ok(String::from_utf8(decoded).expect("decoded segments are ASCII"))
}

fn hex_value(byte: u8) -> Option<u8> {
    match byte {
        b'0'..=b'9' => Some(byte - b'0'),
        b'A'..=b'F' => Some(byte - b'A' + 10),
        _ => None,
    }
}

fn is_unreserved(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'_' | b'~' | b'-')
}