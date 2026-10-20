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
    rank: Vec<u8>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
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
        let head = &path[..head_end];
        let segments = parse_path(head)?;

        let mut best: Option<(usize, Vec<(String, String)>)> = None;

        for (route_id, route) in self.routes.iter().enumerate() {
            if route.segments.len() != segments.len() {
                continue;
            }

            let mut params = Vec::new();
            let mut matched = true;

            for (pattern_segment, path_segment) in route.segments.iter().zip(&segments) {
                match pattern_segment {
                    Segment::Static(value) if value == path_segment => {}
                    Segment::Static(_) => {
                        matched = false;
                        break;
                    }
                    Segment::Param(name) => {
                        params.push((name.clone(), path_segment.clone()));
                    }
                }
            }

            if matched {
                match best {
                    None => best = Some((route_id, params)),
                    Some((best_id, _)) => {
                        if route.rank > self.routes[best_id].rank {
                            best = Some((route_id, params));
                        }
                    }
                }
            }
        }

        best.map(|(route, params)| Match { route, params })
            .ok_or(Error::NotFound)
    }
}

impl Default for Router {
    fn default() -> Self {
        Self::new()
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
    let mut result = Vec::new();
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

        let segment = &pattern[start..end];
        let first = bytes[start];

        if first == b':' {
            let name_bytes = &bytes[start + 1..end];
            let valid = !name_bytes.is_empty()
                && name_bytes[0].is_ascii_lowercase()
                && name_bytes[1..]
                    .iter()
                    .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || *b == b'_');

            if !valid {
                return Err(Error::InvalidPattern { at: start });
            }

            let name = segment[1..].to_string();
            if !names.insert(name.clone()) {
                return Err(Error::InvalidPattern { at: start });
            }
            result.push(Segment::Param(name));
        } else if first == b'*' {
            return Err(Error::InvalidPattern { at: start });
        } else {
            let decoded = decode_static_segment(segment, start)
                .map_err(|at| Error::InvalidPattern { at })?;
            result.push(Segment::Static(decoded));
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

        let segment = &path[start..end];
        let decoded =
            decode_static_segment(segment, start).map_err(|at| Error::InvalidPath { at })?;
        result.push(decoded);

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

fn decode_static_segment(segment: &str, base: usize) -> Result<String, usize> {
    let bytes = segment.as_bytes();
    let mut decoded = Vec::with_capacity(bytes.len());
    let mut i = 0;

    while i < bytes.len() {
        let byte = bytes[i];

        if byte == b'%' {
            if i + 2 >= bytes.len() {
                return Err(base + i);
            }

            let Some(high) = uppercase_hex(bytes[i + 1]) else {
                return Err(base + i);
            };
            let Some(low) = uppercase_hex(bytes[i + 2]) else {
                return Err(base + i);
            };
            let value = (high << 4) | low;

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