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
    Wildcard(String),
}

#[derive(Debug, Clone)]
struct Route {
    segments: Vec<Segment>,
    rank: Vec<u8>,
    id: usize,
}

pub struct Router {
    routes: Vec<Route>,
}

#[derive(Debug)]
enum ParseError {
    Invalid(usize),
}

impl Router {
    pub fn new() -> Self {
        Self { routes: Vec::new() }
    }

    pub fn add(&mut self, pattern: &str) -> Result<usize, Error> {
        let segments = parse_pattern(pattern).map_err(|e| match e {
            ParseError::Invalid(at) => Error::InvalidPattern { at },
        })?;

        let id = self.routes.len();
        let mut rank = Vec::with_capacity(segments.len() + 1);
        let mut has_wildcard = false;

        for segment in &segments {
            match segment {
                Segment::Static(_) => rank.push(2),
                Segment::Param(_) => rank.push(1),
                Segment::Wildcard(_) => {
                    rank.push(0);
                    has_wildcard = true;
                    break;
                }
            }
        }

        if !has_wildcard {
            rank.push(3);
        }

        self.routes.push(Route { segments, rank, id });
        Ok(id)
    }

    pub fn resolve(&self, path: &str) -> Result<Match, Error> {
        let head_end = path
            .char_indices()
            .find_map(|(i, c)| if c == '?' || c == '#' { Some(i) } else { None })
            .unwrap_or(path.len());
        let head = &path[..head_end];

        let parts = parse_path(head).map_err(|e| match e {
            ParseError::Invalid(at) => Error::InvalidPath { at },
        })?;

        let mut best: Option<(&Route, Vec<(String, String)>)> = None;

        for route in &self.routes {
            if let Some(params) = match_route(&route.segments, &parts) {
                let replace = match &best {
                    None => true,
                    Some((best_route, _)) => {
                        route.rank > best_route.rank
                            || (route.rank == best_route.rank && route.id < best_route.id)
                    }
                };
                if replace {
                    best = Some((route, params));
                }
            }
        }

        best.map(|(route, params)| Match {
            route: route.id,
            params,
        })
        .ok_or(Error::NotFound)
    }
}

fn parse_pattern(input: &str) -> Result<Vec<Segment>, ParseError> {
    if !input.starts_with('/') {
        return Err(ParseError::Invalid(0));
    }
    if input == "/" {
        return Ok(Vec::new());
    }

    let bytes = input.as_bytes();
    let mut raw_segments = split_segments(input);
    let trailing = input.ends_with('/');

    if trailing {
        raw_segments.pop();
    }

    let mut segments = Vec::with_capacity(raw_segments.len());
    let mut names = std::collections::HashSet::new();

    for (start, end) in raw_segments {
        if start == end {
            return Err(ParseError::Invalid(start));
        }
        let raw = &input[start..end];

        if raw.starts_with(':') || raw.starts_with('*') {
            let wildcard = raw.starts_with('*');
            let name = &raw[1..];
            if !valid_name(name) {
                return Err(ParseError::Invalid(start));
            }
            if !names.insert(name.to_string()) {
                return Err(ParseError::Invalid(start));
            }
            if wildcard && end != input.len() - usize::from(trailing) {
                return Err(ParseError::Invalid(start));
            }
            segments.push(if wildcard {
                Segment::Wildcard(name.to_string())
            } else {
                Segment::Param(name.to_string())
            });
        } else {
            let decoded = decode_segment(raw, start)?;
            segments.push(Segment::Static(decoded));
        }
    }

    if trailing {
        return Err(ParseError::Invalid(bytes.len() - 1));
    }

    Ok(segments)
}

fn parse_path(input: &str) -> Result<Vec<String>, ParseError> {
    if !input.starts_with('/') {
        return Err(ParseError::Invalid(0));
    }
    if input == "/" {
        return Ok(Vec::new());
    }

    let mut raw_segments = split_segments(input);
    let trailing = input.ends_with('/');

    if trailing {
        raw_segments.pop();
    }

    let mut segments = Vec::with_capacity(raw_segments.len());
    for (start, end) in raw_segments {
        if start == end {
            return Err(ParseError::Invalid(start));
        }
        segments.push(decode_segment(&input[start..end], start)?);
    }

    if trailing {
        return Err(ParseError::Invalid(input.len() - 1));
    }

    Ok(segments)
}

fn split_segments(input: &str) -> Vec<(usize, usize)> {
    let bytes = input.as_bytes();
    let mut result = Vec::new();
    let mut start = 1;

    for i in 1..bytes.len() {
        if bytes[i] == b'/' {
            result.push((start, i));
            start = i + 1;
        }
    }
    result.push((start, bytes.len()));
    result
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

fn decode_segment(raw: &str, offset: usize) -> Result<String, ParseError> {
    let bytes = raw.as_bytes();
    let mut decoded = Vec::with_capacity(bytes.len());
    let mut i = 0;

    while i < bytes.len() {
        let b = bytes[i];
        if b == b'%' {
            if i + 2 >= bytes.len() {
                return Err(ParseError::Invalid(offset + i));
            }
            let hi = upper_hex(bytes[i + 1]);
            let lo = upper_hex(bytes[i + 2]);
            let Some(hi) = hi else {
                return Err(ParseError::Invalid(offset + i));
            };
            let Some(lo) = lo else {
                return Err(ParseError::Invalid(offset + i));
            };
            let value = (hi << 4) | lo;
            if !(0x20..=0x7e).contains(&value)
                || value == b'/'
                || is_unreserved(value)
            {
                return Err(ParseError::Invalid(offset + i));
            }
            decoded.push(value);
            i += 3;
        } else {
            if !is_unreserved(b) {
                return Err(ParseError::Invalid(offset + i));
            }
            decoded.push(b);
            i += 1;
        }
    }

    Ok(String::from_utf8(decoded).unwrap())
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