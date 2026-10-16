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

        for segment in &segments {
            match segment {
                Segment::Static(_) => rank.push(2),
                Segment::Param(_) => rank.push(1),
                Segment::Wildcard(_) => rank.push(0),
            }
        }
        if !matches!(segments.last(), Some(Segment::Wildcard(_))) {
            rank.push(3);
        }

        let id = self.routes.len();
        self.routes.push(Route { segments, rank });
        Ok(id)
    }

    pub fn resolve(&self, path: &str) -> Result<Match, Error> {
        let head_end = path
            .char_indices()
            .find_map(|(i, c)| matches!(c, '?' | '#').then_some(i))
            .unwrap_or(path.len());
        let segments = parse_path_head(&path[..head_end])?;

        let mut best: Option<(usize, Vec<u8>, Vec<(String, String)>)> = None;

        for (route_id, route) in self.routes.iter().enumerate() {
            if let Some(params) = match_route(&route.segments, &segments) {
                let replace = match &best {
                    None => true,
                    Some((best_id, best_rank, _)) => {
                        route.rank > *best_rank
                            || (route.rank == *best_rank && route_id < *best_id)
                    }
                };
                if replace {
                    best = Some((route_id, route.rank.clone(), params));
                }
            }
        }

        match best {
            Some((route, _, params)) => Ok(Match { route, params }),
            None => Err(Error::NotFound),
        }
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
    let mut raw_segments = Vec::new();
    let mut start = 1;

    for i in 1..bytes.len() {
        if bytes[i] == b'/' {
            if i == bytes.len() - 1 {
                return Err(Error::InvalidPattern { at: i });
            }
            if i == start {
                return Err(Error::InvalidPattern { at: start });
            }
            raw_segments.push((start, i));
            start = i + 1;
        }
    }
    if start == bytes.len() {
        return Err(Error::InvalidPattern { at: bytes.len() - 1 });
    }
    raw_segments.push((start, bytes.len()));

    let mut segments = Vec::with_capacity(raw_segments.len());
    let mut names = Vec::<String>::new();

    for (index, &(start, end)) in raw_segments.iter().enumerate() {
        let raw = &input[start..end];
        if let Some(name) = raw.strip_prefix(':') {
            if !valid_name(name) {
                return Err(Error::InvalidPattern { at: start });
            }
            if names.iter().any(|n| n == name) {
                return Err(Error::InvalidPattern { at: start });
            }
            names.push(name.to_owned());
            segments.push(Segment::Param(name.to_owned()));
        } else if let Some(name) = raw.strip_prefix('*') {
            if index + 1 != raw_segments.len() {
                return Err(Error::InvalidPattern { at: start });
            }
            if !valid_name(name) || names.iter().any(|n| n == name) {
                return Err(Error::InvalidPattern { at: start });
            }
            names.push(name.to_owned());
            segments.push(Segment::Wildcard(name.to_owned()));
        } else {
            segments.push(Segment::Static(decode_segment(input, start, end, true)?));
        }
    }

    Ok(segments)
}

fn parse_path_head(input: &str) -> Result<Vec<String>, Error> {
    if !input.starts_with('/') {
        return Err(Error::InvalidPath { at: 0 });
    }
    if input == "/" {
        return Ok(Vec::new());
    }

    let bytes = input.as_bytes();
    let mut ranges = Vec::new();
    let mut start = 1;

    for i in 1..bytes.len() {
        if bytes[i] == b'/' {
            if i == bytes.len() - 1 {
                return Err(Error::InvalidPath { at: i });
            }
            if i == start {
                return Err(Error::InvalidPath { at: start });
            }
            ranges.push((start, i));
            start = i + 1;
        }
    }
    if start == bytes.len() {
        return Err(Error::InvalidPath {
            at: bytes.len() - 1,
        });
    }
    ranges.push((start, bytes.len()));

    ranges
        .into_iter()
        .map(|(start, end)| decode_segment(input, start, end, false))
        .collect()
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

fn decode_segment(
    input: &str,
    start: usize,
    end: usize,
    pattern: bool,
) -> Result<String, Error> {
    let bytes = input.as_bytes();
    let mut decoded = Vec::with_capacity(end - start);
    let mut i = start;

    while i < end {
        let b = bytes[i];
        if b == b'%' {
            if i + 2 >= end {
                return invalid_segment(pattern, i);
            }
            let hi = upper_hex(bytes[i + 1]);
            let lo = upper_hex(bytes[i + 2]);
            let (Some(hi), Some(lo)) = (hi, lo) else {
                return invalid_segment(pattern, i);
            };
            let value = (hi << 4) | lo;
            if !(0x20..=0x7e).contains(&value)
                || value == b'/'
                || is_unreserved(value)
            {
                return invalid_segment(pattern, i);
            }
            decoded.push(value);
            i += 3;
        } else {
            if !b.is_ascii() || !is_unreserved(b) {
                return invalid_segment(pattern, i);
            }
            decoded.push(b);
            i += 1;
        }
    }

    // All accepted raw and escaped bytes are ASCII.
    Ok(String::from_utf8(decoded).expect("validated ASCII bytes"))
}

fn invalid_segment<T>(pattern: bool, at: usize) -> Result<T, Error> {
    if pattern {
        Err(Error::InvalidPattern { at })
    } else {
        Err(Error::InvalidPath { at })
    }
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