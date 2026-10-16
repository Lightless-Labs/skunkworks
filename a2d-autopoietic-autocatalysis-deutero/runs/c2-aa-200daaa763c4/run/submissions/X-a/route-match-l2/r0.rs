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

#[derive(Clone)]
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
        let mut rank = Vec::with_capacity(segments.len() + 1);
        let has_wildcard = segments
            .iter()
            .any(|segment| matches!(segment, Segment::Wildcard(_)));

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
            .char_indices()
            .find_map(|(i, c)| if c == '?' || c == '#' { Some(i) } else { None })
            .unwrap_or(path.len());
        let head = &path[..head_end];
        let request_segments = parse_path(head)?;

        let mut best: Option<(usize, Match)> = None;
        for (index, route) in self.routes.iter().enumerate() {
            if let Some(params) = match_route(&route.segments, &request_segments) {
                let replace = match &best {
                    None => true,
                    Some((best_index, _)) => {
                        route.rank > self.routes[*best_index].rank
                    }
                };
                if replace {
                    best = Some((
                        index,
                        Match {
                            route: route.id,
                            params,
                        },
                    ));
                }
            }
        }

        best.map(|(_, found)| found).ok_or(Error::NotFound)
    }
}

fn parse_pattern(input: &str) -> Result<Vec<Segment>, Error> {
    if !input.starts_with('/') {
        return Err(Error::InvalidPattern { at: 0 });
    }
    if input == "/" {
        return Ok(Vec::new());
    }

    let bytes = input.as_bytes();
    let mut parts = Vec::new();
    let mut start = 1;
    let mut names = std::collections::HashSet::new();

    while start < bytes.len() {
        let end = bytes[start..]
            .iter()
            .position(|&b| b == b'/')
            .map(|offset| start + offset)
            .unwrap_or(bytes.len());

        if end == start {
            return Err(Error::InvalidPattern { at: start });
        }

        let raw = &input[start..end];
        let first = bytes[start];

        if first == b':' || first == b'*' {
            let is_wildcard = first == b'*';
            let name = &raw[1..];
            if !valid_name(name) || !names.insert(name.to_owned()) {
                return Err(Error::InvalidPattern { at: start });
            }
            if is_wildcard && end != bytes.len() {
                return Err(Error::InvalidPattern { at: start });
            }
            parts.push(if is_wildcard {
                Segment::Wildcard(name.to_owned())
            } else {
                Segment::Param(name.to_owned())
            });
        } else {
            let decoded = decode_segment(raw, start).map_err(|at| Error::InvalidPattern { at })?;
            parts.push(Segment::Static(decoded));
        }

        if end == bytes.len() {
            break;
        }
        if end + 1 == bytes.len() {
            return Err(Error::InvalidPattern { at: end });
        }
        start = end + 1;
    }

    Ok(parts)
}

fn parse_path(input: &str) -> Result<Vec<String>, Error> {
    if !input.starts_with('/') {
        return Err(Error::InvalidPath { at: 0 });
    }
    if input == "/" {
        return Ok(Vec::new());
    }

    let bytes = input.as_bytes();
    let mut parts = Vec::new();
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

        let decoded =
            decode_segment(&input[start..end], start).map_err(|at| Error::InvalidPath { at })?;
        parts.push(decoded);

        if end == bytes.len() {
            break;
        }
        if end + 1 == bytes.len() {
            return Err(Error::InvalidPath { at: end });
        }
        start = end + 1;
    }

    Ok(parts)
}

fn valid_name(name: &str) -> bool {
    let bytes = name.as_bytes();
    if bytes.is_empty() || !bytes[0].is_ascii_lowercase() {
        return false;
    }
    bytes[1..]
        .iter()
        .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || *b == b'_')
}

fn decode_segment(raw: &str, offset: usize) -> Result<String, usize> {
    let bytes = raw.as_bytes();
    let mut decoded = Vec::with_capacity(bytes.len());
    let mut i = 0;

    while i < bytes.len() {
        let b = bytes[i];
        if b == b'%' {
            if i + 2 >= bytes.len() {
                return Err(offset + i);
            }
            let hi = upper_hex(bytes[i + 1]).ok_or(offset + i)?;
            let lo = upper_hex(bytes[i + 2]).ok_or(offset + i)?;
            let value = (hi << 4) | lo;

            if !(0x20..=0x7e).contains(&value)
                || value == b'/'
                || is_unreserved(value)
            {
                return Err(offset + i);
            }
            decoded.push(value);
            i += 3;
        } else if is_unreserved(b) {
            decoded.push(b);
            i += 1;
        } else {
            return Err(offset + i);
        }
    }

    Ok(String::from_utf8(decoded).expect("decoded segment is ASCII"))
}

fn upper_hex(b: u8) -> Option<u8> {
    match b {
        b'0'..=b'9' => Some(b - b'0'),
        b'A'..=b'F' => Some(b - b'A' + 10),
        _ => None,
    }
}

fn is_unreserved(b: u8) -> bool {
    b.is_ascii_alphanumeric() || matches!(b, b'.' | b'_' | b'~' | b'-')
}

fn match_route(pattern: &[Segment], path: &[String]) -> Option<Vec<(String, String)>> {
    let mut params = Vec::new();
    let mut path_index = 0;

    for segment in pattern {
        match segment {
            Segment::Static(value) => {
                if path.get(path_index)? != value {
                    return None;
                }
                path_index += 1;
            }
            Segment::Param(name) => {
                let value = path.get(path_index)?;
                params.push((name.clone(), value.clone()));
                path_index += 1;
            }
            Segment::Wildcard(name) => {
                let value = path[path_index..].join("/");
                params.push((name.clone(), value));
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