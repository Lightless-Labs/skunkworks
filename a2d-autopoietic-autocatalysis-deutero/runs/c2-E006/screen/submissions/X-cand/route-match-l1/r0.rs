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
    id: usize,
    segments: Vec<Segment>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct DecodedSegment {
    value: String,
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
        self.routes.push(Route { id, segments });
        Ok(id)
    }

    pub fn resolve(&self, path: &str) -> Result<Match, Error> {
        let head_end = path
            .char_indices()
            .find(|(_, ch)| *ch == '?' || *ch == '#')
            .map_or(path.len(), |(i, _)| i);
        let head = &path[..head_end];
        let request_segments = parse_path(head)?;

        let mut best: Option<(&Route, Vec<(String, String)>)> = None;

        for route in &self.routes {
            if route.segments.len() != request_segments.len() {
                continue;
            }

            let mut params = Vec::new();
            let mut matches = true;

            for (pattern_segment, request_segment) in
                route.segments.iter().zip(request_segments.iter())
            {
                match pattern_segment {
                    Segment::Static(value) if value == &request_segment.value => {}
                    Segment::Static(_) => {
                        matches = false;
                        break;
                    }
                    Segment::Param(name) => {
                        params.push((name.clone(), request_segment.value.clone()));
                    }
                }
            }

            if !matches {
                continue;
            }

            let replace = match best {
                None => true,
                Some((best_route, _)) => {
                    rank_is_greater(&route.segments, &best_route.segments)
                }
            };

            if replace {
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

fn rank_is_greater(left: &[Segment], right: &[Segment]) -> bool {
    for (a, b) in left.iter().zip(right.iter()) {
        let a_rank = match a {
            Segment::Static(_) => 2,
            Segment::Param(_) => 1,
        };
        let b_rank = match b {
            Segment::Static(_) => 2,
            Segment::Param(_) => 1,
        };
        if a_rank != b_rank {
            return a_rank > b_rank;
        }
    }
    false
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
    let mut names = std::collections::HashSet::<String>::new();
    let mut start = 1;

    while start < bytes.len() {
        let end = bytes[start..]
            .iter()
            .position(|&b| b == b'/')
            .map_or(bytes.len(), |offset| start + offset);

        if end == start {
            return Err(Error::InvalidPattern { at: start });
        }

        let raw = &bytes[start..end];

        if raw[0] == b':' {
            let name = &raw[1..];
            let valid = !name.is_empty()
                && name[0].is_ascii_lowercase()
                && name[1..]
                    .iter()
                    .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || *b == b'_');
            if !valid {
                return Err(Error::InvalidPattern { at: start });
            }
            let name = String::from_utf8(name.to_vec()).expect("capture names are ASCII");
            if !names.insert(name.clone()) {
                return Err(Error::InvalidPattern { at: start });
            }
            segments.push(Segment::Param(name));
        } else if raw[0] == b'*' {
            return Err(Error::InvalidPattern { at: start });
        } else {
            let decoded = decode_static_segment(raw, start).map_err(|at| Error::InvalidPattern { at })?;
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

fn parse_path(path: &str) -> Result<Vec<DecodedSegment>, Error> {
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
            .map_or(bytes.len(), |offset| start + offset);

        if end == start {
            return Err(Error::InvalidPath { at: start });
        }

        let decoded = decode_static_segment(&bytes[start..end], start)
            .map_err(|at| Error::InvalidPath { at })?;
        segments.push(DecodedSegment { value: decoded });

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

fn decode_static_segment(raw: &[u8], base: usize) -> Result<String, usize> {
    let mut decoded = Vec::with_capacity(raw.len());
    let mut i = 0;

    while i < raw.len() {
        let byte = raw[i];
        if byte == b'%' {
            if i + 2 >= raw.len() {
                return Err(base + i);
            }
            let high = uppercase_hex(raw[i + 1]).ok_or(base + i)?;
            let low = uppercase_hex(raw[i + 2]).ok_or(base + i)?;
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