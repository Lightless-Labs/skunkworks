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
            .find(|c| c == '?' || c == '#')
            .unwrap_or(path.len());
        let head = &path[..head_end];
        let request_segments = parse_path(head)?;

        let mut best: Option<(usize, Vec<String>)> = None;

        for (route_id, route) in self.routes.iter().enumerate() {
            if route.segments.len() != request_segments.len() {
                continue;
            }

            let mut captures = Vec::new();
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
                    Segment::Param(_) => captures.push(request_segment.clone()),
                }
            }

            if !matched {
                continue;
            }

            let replace = match best.as_ref() {
                None => true,
                Some((best_id, _)) => {
                    route.rank > self.routes[*best_id].rank
                        || (route.rank == self.routes[*best_id].rank && route_id < *best_id)
                }
            };

            if replace {
                best = Some((route_id, captures));
            }
        }

        let (route_id, capture_values) = best.ok_or(Error::NotFound)?;
        let mut params = Vec::with_capacity(capture_values.len());
        let mut values = capture_values.into_iter();

        for segment in &self.routes[route_id].segments {
            if let Segment::Param(name) = segment {
                params.push((name.clone(), values.next().unwrap()));
            }
        }

        Ok(Match {
            route: route_id,
            params,
        })
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
        if segment.starts_with(':') {
            let name = &segment[1..];
            if !valid_capture_name(name) || !names.insert(name.to_owned()) {
                return Err(Error::InvalidPattern { at: start });
            }
            result.push(Segment::Param(name.to_owned()));
        } else if segment.starts_with('*') {
            return Err(Error::InvalidPattern { at: start });
        } else {
            result.push(Segment::Static(decode_segment(pattern, start, end, true)?));
        }

        if end == bytes.len() {
            return Ok(result);
        }
        if end + 1 == bytes.len() {
            return Err(Error::InvalidPattern { at: end });
        }
        start = end + 1;
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

        result.push(decode_segment(path, start, end, false)?);

        if end == bytes.len() {
            return Ok(result);
        }
        if end + 1 == bytes.len() {
            return Err(Error::InvalidPath { at: end });
        }
        start = end + 1;
    }

    Ok(result)
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

fn decode_segment(
    input: &str,
    start: usize,
    end: usize,
    pattern: bool,
) -> Result<String, Error> {
    let bytes = input.as_bytes();
    let mut decoded = Vec::new();
    let mut i = start;

    while i < end {
        let b = bytes[i];
        if b == b'%' {
            if i + 2 >= end {
                return invalid_segment(pattern, i);
            }
            let hi = hex_upper(bytes[i + 1]).ok_or_else(|| segment_error(pattern, i))?;
            let lo = hex_upper(bytes[i + 2]).ok_or_else(|| segment_error(pattern, i))?;
            let value = (hi << 4) | lo;
            if !(0x20..=0x7e).contains(&value)
                || value == b'/'
                || is_unreserved(value)
            {
                return invalid_segment(pattern, i);
            }
            decoded.push(value);
            i += 3;
        } else if is_unreserved(b) {
            decoded.push(b);
            i += 1;
        } else {
            return invalid_segment(pattern, i);
        }
    }

    Ok(String::from_utf8(decoded).unwrap())
}

fn hex_upper(b: u8) -> Option<u8> {
    match b {
        b'0'..=b'9' => Some(b - b'0'),
        b'A'..=b'F' => Some(b - b'A' + 10),
        _ => None,
    }
}

fn is_unreserved(b: u8) -> bool {
    b.is_ascii_alphanumeric() || matches!(b, b'.' | b'_' | b'~' | b'-')
}

fn segment_error(pattern: bool, at: usize) -> Error {
    if pattern {
        Error::InvalidPattern { at }
    } else {
        Error::InvalidPath { at }
    }
}

fn invalid_segment<T>(pattern: bool, at: usize) -> Result<T, Error> {
    Err(segment_error(pattern, at))
}

fn main() {}