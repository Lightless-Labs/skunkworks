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

#[derive(Clone)]
struct Route {
    segments: Vec<Segment>,
    id: usize,
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
        self.routes.push(Route { segments, id });
        Ok(id)
    }

    pub fn resolve(&self, path: &str) -> Result<Match, Error> {
        let head_end = path
            .char_indices()
            .find_map(|(i, c)| if c == '?' || c == '#' { Some(i) } else { None })
            .unwrap_or(path.len());
        let head = &path[..head_end];
        let request_segments = parse_path(head)?;

        let mut best: Option<(usize, Vec<(String, String)>)> = None;
        let mut best_rank: Vec<u8> = Vec::new();

        for route in &self.routes {
            if route.segments.len() != request_segments.len() {
                continue;
            }

            let mut rank = Vec::with_capacity(route.segments.len());
            let mut params = Vec::new();
            let mut matched = true;

            for (pattern_segment, value) in route.segments.iter().zip(&request_segments) {
                match pattern_segment {
                    Segment::Static(expected) => {
                        rank.push(2);
                        if expected != value {
                            matched = false;
                            break;
                        }
                    }
                    Segment::Param(name) => {
                        rank.push(1);
                        params.push((name.clone(), value.clone()));
                    }
                }
            }

            if matched && (best.is_none() || rank > best_rank) {
                best_rank = rank;
                best = Some((route.id, params));
            }
        }

        match best {
            Some((route, params)) => Ok(Match { route, params }),
            None => Err(Error::NotFound),
        }
    }
}

fn parse_pattern(input: &str) -> Result<Vec<Segment>, Error> {
    let raw_segments = split_segments(input).map_err(|at| Error::InvalidPattern { at })?;
    let mut result = Vec::with_capacity(raw_segments.len());
    let mut names: Vec<String> = Vec::new();

    for (segment, start) in raw_segments {
        if segment.starts_with(':') {
            let name = &segment[1..];
            if !valid_capture_name(name) || names.iter().any(|n| n == name) {
                return Err(Error::InvalidPattern { at: start });
            }
            let owned = name.to_owned();
            names.push(owned.clone());
            result.push(Segment::Param(owned));
        } else if segment.starts_with('*') {
            return Err(Error::InvalidPattern { at: start });
        } else {
            let decoded = decode_static(segment, start)
                .map_err(|at| Error::InvalidPattern { at })?;
            result.push(Segment::Static(decoded));
        }
    }

    Ok(result)
}

fn parse_path(input: &str) -> Result<Vec<String>, Error> {
    let raw_segments = split_segments(input).map_err(|at| Error::InvalidPath { at })?;
    let mut result = Vec::with_capacity(raw_segments.len());

    for (segment, start) in raw_segments {
        let decoded =
            decode_static(segment, start).map_err(|at| Error::InvalidPath { at })?;
        result.push(decoded);
    }

    Ok(result)
}

fn split_segments(input: &str) -> Result<Vec<(&str, usize)>, usize> {
    if !input.starts_with('/') {
        return Err(0);
    }
    if input == "/" {
        return Ok(Vec::new());
    }

    let bytes = input.as_bytes();
    let mut result = Vec::new();
    let mut segment_start = 1;

    for i in 1..bytes.len() {
        if bytes[i] == b'/' {
            if i == segment_start {
                return Err(i);
            }
            result.push((&input[segment_start..i], segment_start));
            segment_start = i + 1;
        }
    }

    if segment_start == bytes.len() {
        return Err(bytes.len() - 1);
    }
    result.push((&input[segment_start..], segment_start));
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

fn decode_static(segment: &str, base: usize) -> Result<String, usize> {
    let bytes = segment.as_bytes();
    let mut decoded = Vec::with_capacity(bytes.len());
    let mut i = 0;

    while i < bytes.len() {
        let byte = bytes[i];
        if byte == b'%' {
            if i + 2 >= bytes.len()
                || !is_upper_hex(bytes[i + 1])
                || !is_upper_hex(bytes[i + 2])
            {
                return Err(base + i);
            }
            let value = (hex_value(bytes[i + 1]) << 4) | hex_value(bytes[i + 2]);
            if !(0x20..=0x7e).contains(&value)
                || value == b'/'
                || is_unreserved(value)
            {
                return Err(base + i);
            }
            decoded.push(value);
            i += 3;
        } else {
            if !is_unreserved(byte) {
                return Err(base + i);
            }
            decoded.push(byte);
            i += 1;
        }
    }

    Ok(String::from_utf8(decoded).expect("decoded bytes are ASCII"))
}

fn is_unreserved(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'_' | b'~' | b'-')
}

fn is_upper_hex(byte: u8) -> bool {
    byte.is_ascii_digit() || (b'A'..=b'F').contains(&byte)
}

fn hex_value(byte: u8) -> u8 {
    match byte {
        b'0'..=b'9' => byte - b'0',
        b'A'..=b'F' => byte - b'A' + 10,
        _ => 0,
    }
}

fn main() {}